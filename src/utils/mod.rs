use std::{thread, time::Duration};

use usvg::TreeParsing;
use layers::skia;
pub mod natural_layout;

pub fn image_from_svg(image_data: &[u8], ctx: Option<skia::gpu::DirectContext>) -> layers::skia::Image {
    let options = usvg::Options::default();
    let mut rtree = usvg::Tree::from_data(image_data, &options).unwrap();
    rtree.size = usvg::Size::from_wh(512.0, 512.0).unwrap();
    let xml_options = usvg::XmlOptions::default();
    let xml = usvg::TreeWriting::to_string(&rtree, &xml_options);
    let font_mgr = layers::skia::FontMgr::new();
    let svg = layers::skia::svg::Dom::from_bytes(xml.as_bytes(), font_mgr).unwrap();

    let mut surface = {
        if let Some(mut ctx) = ctx {
            let image_info = skia::ImageInfo::new(
                (512, 512),
                skia::ColorType::RGBA8888,
                skia::AlphaType::Premul,
                None,
            );
            skia::gpu::surfaces::render_target(
                &mut ctx,
                skia::gpu::Budgeted::No,
                &image_info,
                None,
                skia::gpu::SurfaceOrigin::TopLeft,
                None,
                false,
                false,
            )
            .unwrap()
        } else {
            skia::surfaces::raster_n32_premul((512, 512)).unwrap()
        }
    };
    
    let canvas = surface.canvas();
    svg.render(canvas);
    surface.image_snapshot()
}
pub fn image_from_path(image_path: &str, ctx: Option<skia::gpu::DirectContext>) -> Option<layers::skia::Image> {
    let image_path = std::path::Path::new(image_path);
    let image_data = std::fs::read(image_path).ok()?;

    let image = if image_path.extension().and_then(std::ffi::OsStr::to_str) == Some("svg") {
        image_from_svg(&image_data, ctx)
    } else {
        layers::skia::Image::from_encoded(layers::skia::Data::new_copy(image_data.as_slice())).unwrap()
    };

    Some(image)
}

pub trait Observable<T> {
    fn add_listener(&mut self, observer: std::sync::Arc<dyn Observer<T>>);
    fn observers<'a>(&'a self) -> Box<dyn Iterator<Item = std::sync::Weak<dyn Observer<T>>> + 'a>;
    fn notify_observers(&self, event: &T) {
        for observer in self.observers() {
            if let Some(observer) = observer.upgrade() {
                observer.notify(event);
            }
        }
    }
}

pub trait Observer<T>: Sync + Send {
    fn notify(&self, event: &T);
}

pub fn acquire_write_lock_with_retry<T>(
    lock: &std::sync::RwLock<T>,
) -> Option<std::sync::RwLockWriteGuard<T>> {
    const MAX_RETRIES: usize = 5;
    const RETRY_DELAY_MS: u64 = 100;
    for _ in 0..MAX_RETRIES {
        if let Ok(guard) = lock.write() {
            return Some(guard);
        }
        thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
    }
    None
}
