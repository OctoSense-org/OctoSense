//! Native float observations are retained exactly; writes use the offered finite choices.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub struct NativeFloat(u32);
impl serde::Serialize for NativeFloat {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire())
    }
}
impl NativeFloat {
    pub fn decode(text:&str,balance:bool)->Option<Self>{
        if text.is_empty()||text.len()>32||!text.bytes().all(|v|v.is_ascii_digit()||b".-+eE".contains(&v)){return None;}
        let value=text.parse::<f32>().ok()?;
        (value.is_finite()&&if balance{(-1.0..=1.0).contains(&value)}else{value>0.0}).then(||Self(value.to_bits()))
    }
    pub fn get(self)->f32{f32::from_bits(self.0)}
    pub fn wire(self)->String{format!("{:?}",self.get())}
    pub fn preset(scale:f32)->Self{Self(scale.to_bits())}
    pub fn balance_label(self)->String{let value=self.get();if value==0.0{"Centered".into()}else{format!("{}% {}",value.abs()*100.0,if value<0.0{"left"}else{"right"})}}
}
pub const CAPTION_SCALES:[f32;5]=[0.25,0.5,1.0,1.5,2.0];
