//! The trusted Settings accessibility vocabulary, separate from Android settings APIs.
use makepad_strict_json::{obj, s, Value};
use std::{collections::HashMap, sync::atomic::{AtomicU32, Ordering}};

pub const MAX_NODES: usize = 256;
pub const MAX_TEXT: usize = 2048;
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

fn next_id() -> Option<i32> {
    NEXT_ID.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id|
        (id < i32::MAX as u32).then_some(id + 1)).ok().map(|id| id as i32)
}

/// IDs are process-monotonic: an Android node retained by an accessibility
/// service must never point at a different semantic target after recycling.
#[derive(Default)]
pub struct Identities { page: String, token: String, nodes: HashMap<String, i32> }
impl Identities {
    pub fn page(&mut self, page: String) {
        if self.page != page || self.token.is_empty() {
            self.page = page;
            self.token = next_id().map(|id| id.to_string()).unwrap_or_default();
            self.nodes.clear();
        }
    }
    pub fn retire(&mut self) { self.page.clear(); self.token.clear(); self.nodes.clear(); }
    pub fn token(&self) -> &str { &self.token }
    pub fn id(&mut self, semantic: String) -> Option<i32> {
        if self.token.is_empty() { return None; }
        if let Some(id) = self.nodes.get(&semantic) { return Some(*id); }
        // Large catalogs can be browsed indefinitely. Dropping old mappings
        // only allocates new IDs; it never assigns an old ID to another item.
        if self.nodes.len() >= 4096 { self.nodes.clear(); }
        let id = next_id()?;
        self.nodes.insert(semantic, id);
        Some(id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind { Click, Focus, SetText, ScrollForward, ScrollBackward }
impl ActionKind {
    pub fn wire(self) -> &'static str { match self { Self::Click=>"click", Self::Focus=>"focus", Self::SetText=>"set_text", Self::ScrollForward=>"scroll_forward", Self::ScrollBackward=>"scroll_backward" } }
}
#[derive(Clone, Debug)]
pub struct Action { pub request_id: i64, pub token: String, pub id: i32, pub target: String, pub kind: ActionKind, pub text: Option<String> }
impl Action {
    pub fn decode(value: &Value) -> Option<Self> {
        let Value::Obj(fields) = value else { return None; };
        if fields.iter().any(|(key,_)| !["schema","request_id","token","id","target","action","text"].contains(&key.as_str())) { return None; }
        if value.get("schema")?.as_i64()? != 1 { return None; }
        let request_id=value.get("request_id")?.as_i64().filter(|v|*v>0)?;
        let id=i32::try_from(value.get("id")?.as_i64()?).ok().filter(|v|*v>0)?;
        let token=value.get("token")?.as_str().filter(|v|!v.is_empty()&&v.len()<=256&&v.bytes().all(|c|(33..=126).contains(&c)))?.to_owned();
        let target=value.get("target")?.as_str().filter(|v|!v.is_empty()&&v.len()<=20&&v.bytes().all(|c|c.is_ascii_digit())&&(*v=="0"||!v.starts_with('0')))?.to_owned();
        let kind=match value.get("action")?.as_str()? { "click"=>ActionKind::Click,"focus"=>ActionKind::Focus,"set_text"=>ActionKind::SetText,"scroll_forward"=>ActionKind::ScrollForward,"scroll_backward"=>ActionKind::ScrollBackward,_=>return None };
        let text=if kind==ActionKind::SetText {Some(value.get("text")?.as_str().filter(|v|v.chars().count()<=MAX_TEXT&&v.chars().all(|c|!c.is_control()||matches!(c,'\n'|'\t')))?.to_owned())} else {if value.get("text").is_some(){return None;} None};
        Some(Self{request_id,token,id,target,kind,text})
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role { Button, Text, Edit }
impl Role { fn wire(self)->&'static str {match self {Self::Button=>"button",Self::Text=>"text",Self::Edit=>"edit"}} }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bounds(pub [i64;4]);
impl Bounds {
    pub fn from_rect(rect: makepad_widgets::Rect, dpi: f64, parent: Option<Self>) -> Option<Self> {
        let edges=[rect.pos.x*dpi,rect.pos.y*dpi,(rect.pos.x+rect.size.x)*dpi,(rect.pos.y+rect.size.y)*dpi];
        if !dpi.is_finite()||dpi<=0.||edges.iter().any(|v|!v.is_finite()||v.abs()>32768.) {return None;}
        let [mut left,mut top,mut right,mut bottom]=edges.map(|v|v.round() as i64);
        if let Some(Self([x,y,w,h]))=parent {left=left.max(x);top=top.max(y);right=right.min(x+w);bottom=bottom.min(y+h);}
        (right>left&&bottom>top&&right-left<=32768&&bottom-top<=32768).then_some(Self([left,top,right-left,bottom-top]))
    }
    fn value(self)->Value {Value::Arr(self.0.into_iter().map(Value::Int).collect())}
}
#[derive(Clone, Debug)]
pub struct Node { pub id:i32,pub parent:Option<i32>,pub role:Role,pub label:String,pub value:String,pub enabled:bool,pub focused:bool,pub actions:Vec<ActionKind>,pub bounds:Bounds }
impl Node {
    pub fn target(&self)->String {self.id.to_string()}
    fn value(&self)->Value {obj(vec![("id",Value::Int(self.id as i64)),("target",s(&self.target())),("parent",self.parent.map_or(Value::Null,|id|Value::Int(id as i64))),
        ("role",s(self.role.wire())),("label",s(&self.label)),("value",s(&self.value)),("enabled",Value::Bool(self.enabled)),("focused",Value::Bool(self.focused)),
        ("actions",Value::Arr(self.actions.iter().map(|action|s(action.wire())).collect())),("bounds",self.bounds.value())])}
}
#[derive(Clone, Debug)]
pub struct Scroll {pub id:i32,pub bounds:Bounds,pub forward:bool,pub backward:bool}
impl Scroll {fn value(&self)->Value {obj(vec![("id",Value::Int(self.id as i64)),("target",s(&self.id.to_string())),("bounds",self.bounds.value()),("forward",Value::Bool(self.forward)),("backward",Value::Bool(self.backward))])}}
#[derive(Clone, Debug, Default)]
pub struct Layout {pub token:String,pub pane:String,pub bounds:Bounds,pub scroll:Option<Scroll>,pub nodes:Vec<Node>}
impl Layout {
    pub fn active(&self)->bool {!self.token.is_empty()&&!self.pane.is_empty()&&self.bounds.0[2]>0&&self.bounds.0[3]>0}
    pub fn json(&self)->String {obj(vec![("schema",Value::Int(1)),("active",Value::Bool(self.active())),("token",s(&self.token)),("pane",s(&self.pane)),("bounds",self.bounds.value()),
        ("scroll",self.scroll.as_ref().map_or(Value::Null,Scroll::value)),("nodes",Value::Arr(self.nodes.iter().map(Node::value).collect()))]).to_json()}
    pub fn permits(&self,action:&Action)->bool {
        if !self.active()||self.token!=action.token||action.target!=action.id.to_string(){return false;}
        match action.kind {
            ActionKind::ScrollForward|ActionKind::ScrollBackward=>self.scroll.as_ref().is_some_and(|scroll|scroll.id==action.id&&if action.kind==ActionKind::ScrollForward{scroll.forward}else{scroll.backward}),
            _=>self.nodes.iter().any(|node|node.id==action.id&&node.enabled&&node.actions.contains(&action.kind)),
        }
    }
}
pub fn bounded(text:&str)->String {text.chars().filter(|c|!c.is_control()||matches!(c,'\n'|'\t')).take(MAX_TEXT).collect()}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_ids_survive_polls_but_never_rebind_recycled_rows_or_retired_pages() {
        let mut ids=Identities::default();ids.page("apps".into());let token=ids.token().to_owned();
        let a=ids.id("slot0:package.a".into()).unwrap();assert_eq!(ids.id("slot0:package.a".into()),Some(a));
        let b=ids.id("slot0:package.b".into()).unwrap();assert_ne!(a,b);assert_eq!(token,ids.token());
        ids.page("details:package.b".into());assert_ne!(token,ids.token());
        ids.page("apps".into());assert_ne!(ids.id("slot0:package.a".into()),Some(a));
        ids.retire();assert!(ids.token().is_empty());assert!(ids.id("anything".into()).is_none());
    }
    #[test]
    fn actions_require_exact_page_node_and_current_available_operation() {
        let mut layout=Layout{token:"1".into(),pane:"Sound".into(),bounds:Bounds([0,0,100,100]),nodes:vec![Node{id:2,parent:None,role:Role::Button,label:"Increase media volume".into(),value:String::new(),enabled:true,focused:false,actions:vec![ActionKind::Click],bounds:Bounds([0,0,10,10])}],scroll:None};
        let mut action=Action{request_id:1,token:"1".into(),id:2,target:"2".into(),kind:ActionKind::Click,text:None};assert!(layout.permits(&action));
        action.id=3;assert!(!layout.permits(&action));action.id=2;action.token="old".into();assert!(!layout.permits(&action));action.token="1".into();
        layout.nodes[0].enabled=false;assert!(!layout.permits(&action));layout.nodes[0].enabled=true;action.kind=ActionKind::SetText;assert!(!layout.permits(&action));
        assert!(!Layout::default().permits(&action));
    }
    #[test]
    fn android_bounds_round_clip_and_text_export_obey_native_contract() {
        use makepad_widgets::{Rect,dvec2};
        let parent=Bounds::from_rect(Rect{pos:dvec2(1.2,2.4),size:dvec2(100.3,50.5)},2.625,None).unwrap();
        let child=Bounds::from_rect(Rect{pos:dvec2(0.,0.),size:dvec2(105.,60.)},2.625,Some(parent)).unwrap();assert_eq!(child,parent);
        assert!(Bounds::from_rect(Rect{pos:dvec2(32769.,0.),size:dvec2(1.,1.)},1.,None).is_none());
        assert!(Bounds::from_rect(Rect{pos:dvec2(-20000.,0.),size:dvec2(40000.,1.)},1.,None).is_none());
        assert_eq!(bounded("Mail\u{0}\u{7}\u{85}\n中文\t🙂"),"Mail\n中文\t🙂");
        assert_eq!(bounded(&"🙂".repeat(3000)).chars().count(),MAX_TEXT);
    }

    #[test]
    fn action_decoder_rejects_unknown_actions_keys_invalid_ids_tokens_and_editor_payloads() {
        let base=vec![("schema",Value::Int(1)),("request_id",Value::Int(1)),("token",s("1")),("id",Value::Int(2)),("target",s("2")),("action",s("click"))];
        assert!(Action::decode(&obj(base.clone())).is_some());
        for (key,value) in [("schema",Value::Int(2)),("request_id",Value::Int(0)),("id",Value::Int(i32::MAX as i64+1)),("token",s("with space")),("target",s("02")),("action",s("shell"))] {
            let mut fields=base.clone();fields.retain(|(k,_)|*k!=key);fields.push((key,value));assert!(Action::decode(&obj(fields)).is_none(),"{key}");
        }
        let mut fields=base.clone();fields.push(("text",s("unexpected")));assert!(Action::decode(&obj(fields)).is_none());
        let mut fields=base.clone();fields.push(("package",s("arbitrary.app")));assert!(Action::decode(&obj(fields)).is_none());
        let mut fields=base;fields.retain(|(k,_)|*k!="action");fields.push(("action",s("set_text")));assert!(Action::decode(&obj(fields.clone())).is_none());
        fields.push(("text",s("中文🙂")));assert!(Action::decode(&obj(fields.clone())).is_some());
        fields.pop();fields.push(("text",s("\u{0}")));assert!(Action::decode(&obj(fields)).is_none());
    }

}
