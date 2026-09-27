//! The Python runner inserts the actual installed TextInput setters and sync path.
use std::ops::Range;
#[derive(Clone, Copy)] struct Area(bool);
impl Area { fn is_empty(self)->bool {!self.0} }
#[derive(Clone, Copy)] struct Cursor { index:usize, prefer_next_row:bool }
#[derive(Clone, Copy)] struct Selection { anchor:Cursor, cursor:Cursor }
impl Selection {
    fn start(&self)->Cursor {if self.anchor.index<self.cursor.index {self.anchor}else{self.cursor}}
    fn end(&self)->Cursor {if self.anchor.index>self.cursor.index {self.anchor}else{self.cursor}}
}
#[derive(Clone, Copy, Debug, PartialEq)] struct CharOffset(usize);
#[derive(Default)] struct Cx { focused:bool, sent:Vec<(String,Range<CharOffset>,Option<Range<CharOffset>>)> }
impl Cx {
    fn has_key_focus(&self,area:Area)->bool {self.focused&&area.0}
    fn sync_ime_state(&mut self,text:String,selection:Range<CharOffset>,composition:Option<Range<CharOffset>>) {self.sent.push((text,selection,composition));}
}
struct Background(Area);
impl Background {fn area(&self)->Area {self.0} fn redraw(&mut self,_cx:&mut Cx){}}
struct History;
impl History {fn clear(&mut self){} fn force_new_edit_group(&mut self){}}
fn floor_grapheme_boundary(text:&str,mut index:usize)->usize {index=index.min(text.len());while !text.is_char_boundary(index) {index-=1;}index}
struct TextInput {
    text:String,selection:Selection,history:History,draw_bg:Background,is_read_only:bool,
    needs_scroll_to_cursor:bool,laidout_text:Option<()>,composition_start:usize,composition_end:usize,
    last_sent_ime_text:String,last_sent_ime_sel_start:usize,last_sent_ime_sel_end:usize,
}
impl TextInput {
    fn filter_input(&self,text:&str,_programmatic:bool)->String {text.into()}
    fn check_text_is_empty(&mut self,_cx:&mut Cx){}
    /* REAL_EDITOR_METHODS */
}
fn main() {
    let end=Cursor{index:9,prefer_next_row:false};
    let mut input=TextInput{text:"bluetooth".into(),selection:Selection{anchor:end,cursor:end},history:History,draw_bg:Background(Area(true)),is_read_only:false,needs_scroll_to_cursor:false,laidout_text:None,composition_start:0,composition_end:0,last_sent_ime_text:"bluetooth".into(),last_sent_ime_sel_start:9,last_sent_ime_sel_end:9};
    let mut cx=Cx{focused:true,..Default::default()};
    input.set_text(&mut cx,"");
    assert_eq!(cx.sent.len(),1,"Clear must synchronize before another input, without an untracked host write");
    assert_eq!(cx.sent[0],("".into(),CharOffset(0)..CharOffset(0),None));
    // Java may type a new letter between the setter and the next focused draw.
    // The draw's existing path must not send another empty buffer over it.
    input.update_ime_context(&mut cx);
    assert_eq!(cx.sent.len(),1,"the next draw must not erase newly accepted Java input with a second Clear");
    input.set_text(&mut cx,"邮件 abc");cx.sent.clear();
    input.set_selection(&mut cx,Selection{anchor:Cursor{index:0,prefer_next_row:false},cursor:Cursor{index:input.text.len(),prefer_next_row:false}});
    assert_eq!(cx.sent[0].1,CharOffset(0)..CharOffset(6),"selection is synchronized in character offsets before a subsequent replacement");
    input.update_ime_context(&mut cx);assert_eq!(cx.sent.len(),1);
    cx.sent.clear();cx.focused=false;input.set_text(&mut cx,"background");assert!(cx.sent.is_empty(),"background widgets must not overwrite the active editor");
    cx.focused=true;input.is_read_only=true;input.set_text(&mut cx,"read only");assert!(cx.sent.is_empty());
    input.is_read_only=false;input.draw_bg.0=Area(false);input.set_text(&mut cx,"unmounted");assert!(cx.sent.is_empty());
}
