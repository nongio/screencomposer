use std::{
    collections::HashMap,
    sync::{Arc, Once, RwLock},
    thread,
    time::Duration,
};

use lay_rs::{
    prelude::{ContentDrawFunction, Layer, PointerHandlerFunction},
    skia::{self},
    utils::load_svg_image,
};

use crate::{config::Config, workspaces::utils::FONT_CACHE};
pub mod natural_layout;

static INIT: Once = Once::new();
static mut ICON_CACHE: Option<Arc<RwLock<HashMap<String, skia::Image>>>> = None;

fn init_icon_cache() {
    unsafe {
        ICON_CACHE = Some(Arc::new(RwLock::new(HashMap::new())));
    }
}

fn icon_cache() -> Arc<RwLock<HashMap<String, skia::Image>>> {
    let icon_cache = unsafe {
        INIT.call_once(init_icon_cache);
        ICON_CACHE.as_ref().unwrap()
    };
    icon_cache.clone()
}

// FIXME check why skia_safe svg is broken
// pub fn image_from_svg(
//     image_data: &[u8],
//     ctx: Option<skia::gpu::DirectContext>,
// ) -> lay_rs::skia::Image {
//     let options = usvg::Options::default();
//     let mut rtree = usvg::Tree::from_data(image_data, &options).unwrap();
//     rtree.size = usvg::Size::from_wh(512.0, 512.0).unwrap();
//     let xml_options = usvg::XmlOptions::default();
//     let xml = usvg::TreeWriting::to_string(&rtree, &xml_options);
//     let font_mgr = lay_rs::skia::FontMgr::new();
//     let svg = lay_rs::skia::svg::Dom::from_bytes(xml.as_bytes(), font_mgr).unwrap();

//     let mut surface = {
//         if let Some(mut ctx) = ctx {
//             let image_info = skia::ImageInfo::new(
//                 (512, 512),
//                 skia::ColorType::RGBA8888,
//                 skia::AlphaType::Premul,
//                 None,
//             );
//             skia::gpu::surfaces::render_target(
//                 &mut ctx,
//                 skia::gpu::Budgeted::No,
//                 &image_info,
//                 None,
//                 skia::gpu::SurfaceOrigin::TopLeft,
//                 None,
//                 false,
//                 false,
//             )
//             .unwrap()
//         } else {
//             skia::surfaces::raster_n32_premul((512, 512)).unwrap()
//         }
//     };

//     let canvas = surface.canvas();
//     svg.render(canvas);
//     surface.image_snapshot()
// }

pub fn image_from_path(path: &str, size: impl Into<skia::ISize>) -> Option<lay_rs::skia::Image> {
    let image_path = std::path::Path::new(path);

    let image = if image_path.extension().and_then(std::ffi::OsStr::to_str) == Some("svg") {
        load_svg_image(path, size).ok()?
    } else {
        let image_data = std::fs::read(image_path).ok()?;
        lay_rs::skia::Image::from_encoded(lay_rs::skia::Data::new_copy(image_data.as_slice()))
            .unwrap()
    };

    Some(image)
}

pub fn named_icon(icon_name: &str) -> Option<lay_rs::skia::Image> {
    let ic = icon_cache();
    let mut ic = ic.write().unwrap();
    if let Some(icon) = ic.get(icon_name) {
        return Some(icon.clone());
    }
    // not found
    let icon_path = xdgkit::icon_finder::find_icon(icon_name.to_string(), 512, 1)
        .map(|icon| icon.to_str().unwrap().to_string());
    let icon = icon_path
        .as_ref()
        .and_then(|icon_path| image_from_path(icon_path, (512, 512)));
    if let Some(i) = icon.as_ref() {
        ic.insert(icon_name.to_string(), i.clone());
    }
    icon
}
pub fn draw_named_icon(icon_name: &str) -> Option<ContentDrawFunction> {
    let icon = named_icon(icon_name);
    icon.as_ref().map(|icon| {
        let icon = icon.clone();
        let resampler = skia::CubicResampler::catmull_rom();

        let draw_function = move |canvas: &skia::Canvas, w: f32, h: f32| -> lay_rs::skia::Rect {
            let paint = skia::Paint::new(skia::Color4f::new(1.0, 1.0, 1.0, 1.0), None);
            canvas.draw_image_rect_with_sampling_options(
                &icon,
                None,
                skia::Rect::from_xywh(0.0, 0.0, w, h),
                resampler,
                &paint,
            );
            skia::Rect::from_xywh(0.0, 0.0, w, h)
        };
        draw_function.into()
    })
}

pub fn notify_observers<T>(observers: &Vec<std::sync::Weak<dyn Observer<T>>>, event: &T) {
    for observer in observers {
        if let Some(observer) = observer.upgrade() {
            observer.notify(event);
        }
    }
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

pub fn draw_text_content(
    text: impl Into<String>,
    text_style: skia::textlayout::TextStyle,
    text_align: skia::textlayout::TextAlign,
) -> Option<ContentDrawFunction> {
    let text = text.into();
    let foreground_paint =
        lay_rs::skia::Paint::new(lay_rs::skia::Color4f::new(0.0, 0.0, 0.0, 0.5), None);
    let mut text_style = text_style.clone();
    text_style.set_foreground_paint(&foreground_paint);
    let ff = Config::with(|c| c.font_family.clone());
    text_style.set_font_families(&[ff]);

    let mut paragraph_style = lay_rs::skia::textlayout::ParagraphStyle::new();
    paragraph_style.set_text_direction(lay_rs::skia::textlayout::TextDirection::LTR);
    paragraph_style.set_text_style(&text_style.clone());
    paragraph_style.set_text_align(text_align);
    paragraph_style.set_max_lines(1);
    paragraph_style.set_ellipsis("…");
    // println!("FS: {}", text_style.font_size());

    let draw_function = move |canvas: &skia::Canvas, w: f32, h: f32| -> lay_rs::skia::Rect {
        // let paint = skia::Paint::new(skia::Color4f::new(1.0, 1.0, 1.0, 1.0), None);

        let mut builder = FONT_CACHE.with(|font_cache| {
            lay_rs::skia::textlayout::ParagraphBuilder::new(
                &paragraph_style,
                font_cache.font_collection.clone(),
            )
        });
        let mut paragraph = builder.add_text(&text).build();
        paragraph.layout(w);
        paragraph.paint(canvas, (0.0, (h - paragraph.height()) / 2.0));

        skia::Rect::from_xywh(0.0, 0.0, w, h)
    };
    Some(draw_function.into())
}

pub fn button_press_filter() -> PointerHandlerFunction {
    let darken_color = skia::Color::from_argb(100, 100, 100, 100);
    let add = skia::Color::from_argb(0, 0, 0, 0);
    let filter = skia::color_filters::lighting(darken_color, add);

    let f = move |layer: Layer, _x: f32, _y: f32| {
        layer.set_color_filter(filter.clone());
    };
    f.into()
}

pub fn button_release_filter() -> PointerHandlerFunction {
    let f = |layer: Layer, _x: f32, _y: f32| {
        layer.set_color_filter(None);
    };
    f.into()
}
