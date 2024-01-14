use usvg::TreeParsing;


pub fn image_from_svg(image_data: &[u8]) -> skia_safe::Image {
    let options = usvg::Options::default();
    let mut rtree = usvg::Tree::from_data(image_data, &options).unwrap();
    rtree.size = usvg::Size::from_wh(512.0, 512.0).unwrap();
    let xml_options = usvg::XmlOptions::default();
    let xml = usvg::TreeWriting::to_string(&rtree, &xml_options);

    let svg = skia_safe::svg::Dom::from_bytes(xml.as_bytes()).unwrap();

    let mut surface = skia_safe::surface::Surface::new_raster_n32_premul((512, 512)).unwrap();
    let canvas = surface.canvas();
    svg.render(canvas);
    surface.image_snapshot()
}
pub fn image_from_path(image_path: &str) -> skia_safe::Image {
    let image_path = std::path::Path::new(image_path);
    let image_data = std::fs::read(image_path).unwrap();

    let image = if image_path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        == Some("svg")
    {
        image_from_svg(&image_data)
    } else {
        skia_safe::Image::from_encoded(skia_safe::Data::new_copy(image_data.as_slice())).unwrap()
    };

    image
}