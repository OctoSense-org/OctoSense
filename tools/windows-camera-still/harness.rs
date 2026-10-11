//! Portable worker tests. Media Foundation and platform ID types are test doubles;
//! worker, JPEG implementation and frame layout function come from the actual overlay.
#[allow(dead_code)]
mod video {
    pub type VideoInputId = u64;
    #[derive(Clone,Copy,Debug,PartialEq)]
    pub enum VideoPixelFormat { NV12, YUY2, MJPEG, RGB24 }
    #[derive(Clone,Copy,Debug)]
    pub struct VideoFormat { pub width:usize, pub height:usize, pub pixel_format:VideoPixelFormat }
    #[derive(Clone,Copy,Debug)]
    pub enum CameraFrameLayout { NV12, YUY2, Mjpeg }
    #[derive(Clone,Copy,Debug)]
    pub enum CameraColorMatrix { BT709, Unknown }
    #[derive(Clone,Copy,Debug)]
    pub struct CameraFramePlaneRef<'a> { pub bytes:&'a[u8], pub row_stride:usize, pub pixel_stride:usize }
    impl<'a> CameraFramePlaneRef<'a> { pub fn empty()->Self { Self{bytes:&[],row_stride:0,pixel_stride:1} } }
    #[derive(Clone,Copy,Debug)]
    pub struct CameraFrameRef<'a> { pub timestamp_ns:u64,pub width:usize,pub height:usize,pub layout:CameraFrameLayout,pub matrix:CameraColorMatrix,pub plane_count:usize,pub planes:[CameraFramePlaneRef<'a>;3] }
    #[derive(Debug)]
    pub enum CameraCaptureResult { Photo{path:String,width:u32,height:u32}, Failed{what:String,error:String} }
    #[derive(Debug)]
    pub struct CameraCaptureEvent { pub input_id:VideoInputId,pub result:CameraCaptureResult }
}
mod cx {
    use crate::video::CameraCaptureEvent;
    pub static EVENTS:std::sync::Mutex<Vec<CameraCaptureEvent>>=std::sync::Mutex::new(Vec::new());
    pub struct Cx;
    impl Cx { pub fn post_action(event:CameraCaptureEvent) { EVENTS.lock().unwrap().push(event); } }
}
mod windows {
    mod media_foundation {
        use crate::video::*;
        __FRAME_LAYOUT__
    }
    #[path=__WORKER_PATH__]
    mod camera_still;
    use camera_still::StillCapture;
    use crate::video::*;
    use std::{path::PathBuf,sync::atomic::{AtomicU64,Ordering},time::{Duration,Instant}};
    static NEXT:AtomicU64=AtomicU64::new(1);
    fn fixture()->(StillCapture,PathBuf,u64) {
        let id=NEXT.fetch_add(1,Ordering::Relaxed);
        let path=PathBuf::from(std::env::var("OCTOSENSE_STILL_TEST_OUTPUT").unwrap()).join(format!("photo-{id}.jpg"));
        (StillCapture::new(id),path,id)
    }
    fn result(id:u64)->CameraCaptureResult {
        let deadline=Instant::now()+Duration::from_secs(12);
        loop {
            {let mut q=crate::cx::EVENTS.lock().unwrap();if let Some(i)=q.iter().position(|e|e.input_id==id){return q.remove(i).result;}}
            assert!(Instant::now()<deadline,"capture produced no terminal event");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn format(pixel_format:VideoPixelFormat)->VideoFormat { VideoFormat{width:2,height:2,pixel_format} }
    fn decode(bytes:&[u8])->Vec<u8> {
        use makepad_zune_jpeg::{JpegDecoder,makepad_zune_core::bytestream::ZCursor};
        let mut d=JpegDecoder::new(ZCursor::new(bytes));let pixels=d.decode().unwrap();assert_eq!(d.dimensions(),Some((2,2)));pixels
    }
    fn success(r:CameraCaptureResult) { assert!(matches!(r,CameraCaptureResult::Photo{width:2,height:2,..}),"{r:?}"); }
    fn failure(r:CameraCaptureResult,needle:&str) { assert!(matches!(&r,CameraCaptureResult::Failed{error,..} if error.contains(needle)),"{r:?}"); }
    #[test] fn padded_nv12_is_a_decodable_white_jpeg() {
        let (c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();
        c.offer_frame(format(VideoPixelFormat::NV12),&[235,235,0,0,235,235,0,0,128,128,0,0]);
        success(result(id));assert!(decode(&std::fs::read(p).unwrap()).iter().all(|v|*v>=245));c.close();
    }
    #[test] fn yuy2_is_a_decodable_black_jpeg() {
        let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();
        c.offer_frame(format(VideoPixelFormat::YUY2),&[16,128,16,128,16,128,16,128]);
        success(result(id));assert!(decode(&std::fs::read(p).unwrap()).iter().all(|v|*v<=3));c.close();
    }
    #[test] fn mjpeg_is_validated_before_persisting() {
        let mut jpeg=Vec::new();jpeg_encoder::Encoder::new(&mut jpeg,90).encode(&[255,0,0,255,0,0,255,0,0,255,0,0],2,2,jpeg_encoder::ColorType::Rgb).unwrap();
        let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();c.offer_frame(format(VideoPixelFormat::MJPEG),&jpeg);success(result(id));assert_eq!(std::fs::read(p).unwrap(),jpeg);c.close();
        for bytes in [&b"not jpeg"[..],&jpeg[..jpeg.len()/2],&jpeg[..jpeg.len()-8]] { let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();c.offer_frame(format(VideoPixelFormat::MJPEG),bytes);failure(result(id),"JPEG");assert!(!p.exists());c.close(); }
        let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();c.offer_frame(VideoFormat{width:4,..format(VideoPixelFormat::MJPEG)},&jpeg);failure(result(id),"dimensions");assert!(!p.exists());c.close();
    }
    #[test] fn existing_file_is_not_overwritten() {
        let(c,p,id)=fixture();std::fs::write(&p,b"keep this exact file").unwrap();c.request(p.to_string_lossy().into()).unwrap();c.offer_frame(format(VideoPixelFormat::YUY2),&[16,128,16,128,16,128,16,128]);failure(result(id),"existing file");assert_eq!(std::fs::read(p).unwrap(),b"keep this exact file");c.close();
    }
    #[test] fn busy_does_not_replace_the_first_request() {
        let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();let second=p.with_extension("second.jpg");assert!(c.request(second.to_string_lossy().into()).unwrap_err().contains("pending"));c.offer_frame(format(VideoPixelFormat::YUY2),&[16,128,16,128,16,128,16,128]);success(result(id));assert!(p.exists());assert!(!second.exists());c.close();
    }
    #[test] fn cancel_discards_old_frames_and_allows_a_new_request() {
        let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();c.cancel();failure(result(id),"cancelled");c.offer_frame(format(VideoPixelFormat::YUY2),&[16,128,16,128,16,128,16,128]);assert!(!p.exists());c.request(p.to_string_lossy().into()).unwrap();c.offer_frame(format(VideoPixelFormat::YUY2),&[16,128,16,128,16,128,16,128]);success(result(id));c.close();
    }
    #[test] fn closed_camera_settles_and_refuses_new_requests() {
        let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();c.close();failure(result(id),"cancelled");assert!(c.request(p.to_string_lossy().into()).unwrap_err().contains("closed"));assert!(!p.exists());
    }
    #[test] fn missing_frame_times_out_without_a_file() {
        let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();failure(result(id),"timed out");assert!(!p.exists());c.close();
    }
    #[test] fn malformed_oversized_and_unsupported_frames_fail() {
        for (f,bytes,needle) in [(format(VideoPixelFormat::NV12),vec![16,16,16],"layout"),(VideoFormat{width:8192,..format(VideoPixelFormat::NV12)},vec![],"budget"),(format(VideoPixelFormat::RGB24),vec![0;12],"format"),(VideoFormat{width:1,..format(VideoPixelFormat::YUY2)},vec![0;4],"even")] {
            let(c,p,id)=fixture();c.request(p.to_string_lossy().into()).unwrap();c.offer_frame(f,&bytes);failure(result(id),needle);assert!(!p.exists());c.close();
        }
    }
}
