use std::collections::HashMap;

use layers::prelude::taffy;
use smithay::reexports::wayland_server::backend::ObjectId;
use usvg::TreeParsing;

use crate::window_view::WindowView;


pub fn image_from_svg(image_data: &[u8]) -> skia_safe::Image {
    let options = usvg::Options::default();
    let mut rtree = usvg::Tree::from_data(image_data, &options).unwrap();
    rtree.size = usvg::Size::from_wh(512.0, 512.0).unwrap();
    let xml_options = usvg::XmlOptions::default();
    let xml = usvg::TreeWriting::to_string(&rtree, &xml_options);
    let font_mgr = skia_safe::FontMgr::new();
    let svg = skia_safe::svg::Dom::from_bytes(xml.as_bytes(), font_mgr).unwrap();

    let mut surface = skia_safe::surfaces::raster_n32_premul((512, 512)).unwrap();
    let canvas = surface.canvas();
    svg.render(canvas);
    surface.image_snapshot()
}
pub fn image_from_path(image_path: &str) -> Option<skia_safe::Image> {
    let image_path = std::path::Path::new(image_path);
    let image_data = std::fs::read(image_path).ok()?;

    let image = if image_path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        == Some("svg")
    {
        image_from_svg(&image_data)
    } else {
        skia_safe::Image::from_encoded(skia_safe::Data::new_copy(image_data.as_slice())).unwrap()
    };

    Some(image)
}

pub fn bin_pack(window_views: &HashMap<ObjectId, WindowView>, bin_width: f32, bin_height: f32) -> Box<dyn binpack2d::BinPacker> {
    let total_window_area: f32 = {
        window_views
            .iter()
            .map(|(_id, window)| {
                let size = window.base_layer.size();
                match (size.width, size.height) {
                    (taffy::Dimension::Points(width), taffy::Dimension::Points(height)) => {
                        width * height
                    }
                    _ => 0.0,
                }
            })
            .sum()
    };

    let total_bin_area = bin_width * bin_height;
    let mut scale_factor = (total_bin_area / total_window_area).sqrt();
    let mut items_to_place = Vec::new();
    for (_id, window) in window_views.iter() {
        let size = window.base_layer.size();
        let (window_width, window_height) = match (size.width, size.height) {
            (taffy::Dimension::Points(width), taffy::Dimension::Points(height)) => (width, height),
            _ => (0.0, 0.0),
        };
        let id = window.base_layer.id().unwrap();
        let id:usize = id.0.into();
        let dimension = binpack2d::Dimension::with_id(id as isize, (window_width * scale_factor) as i32, (window_height * scale_factor) as i32, 20);
        items_to_place.push(dimension);
    }

    let mut bin = binpack2d::bin_new(binpack2d::BinType::MaxRects, bin_width as i32, bin_height as i32);
    let (mut inserted, mut rejected) = bin.insert_list(&items_to_place);
    let mut tries = 0;
    while (!rejected.is_empty() || inserted.len() != window_views.len()) && tries < 100 {
        scale_factor *= 0.99;
        scale_factor = scale_factor.max(0.1);
        let mut items_to_place = Vec::new();
        for (_id, window) in window_views.iter() {
            let size = window.base_layer.size();
            let (window_width, window_height) = match (size.width, size.height) {
                (taffy::Dimension::Points(width), taffy::Dimension::Points(height)) => (width, height),
                _ => (0.0, 0.0),
            };
            let id = window.base_layer.id().unwrap();
            let id:usize = id.0.into();
            let dimension = binpack2d::Dimension::with_id(id as isize, (window_width * scale_factor) as i32, (window_height * scale_factor) as i32, 20);
            items_to_place.push(dimension);
        }
        bin.clear();
        (inserted, rejected) = bin.insert_list(&items_to_place);
        tries += 1;
    }

    bin
}