use usvg::TreeParsing;


pub mod natural_layout;

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

pub fn bin_pack(windows: &Vec<crate::workspace::Window>, bin_width: f32, bin_height: f32) -> Box<dyn binpack2d::BinPacker>
{
    let total_window_area: f32 = {
        windows.iter()
            .map(|window| {
                window.w * window.h
            })
            .sum()
    };

    let total_bin_area = bin_width * bin_height;
    let mut scale_factor = (total_bin_area / total_window_area).sqrt();
    let mut items_to_place = Vec::new();
    windows.iter().for_each(|win| {
        let (window_width, window_height) = (win.w, win.h);
        let id = win.base_layer.id().unwrap();
        let id: usize = id.0.into();
        let dimension = binpack2d::Dimension::with_id(id as isize, (window_width * scale_factor) as i32, (window_height * scale_factor) as i32, 20);
        items_to_place.push(dimension);
    });

    let mut bin = binpack2d::bin_new(binpack2d::BinType::MaxRects, bin_width as i32, bin_height as i32);
    let (mut inserted, mut rejected) = bin.insert_list(&items_to_place);
    let mut tries = 0;
    while (!rejected.is_empty() || inserted.len() != windows.len()) && tries < 100 {
        scale_factor *= 0.99;
        scale_factor = scale_factor.max(0.1);
        let mut items_to_place = Vec::new();
        windows.iter().for_each(|win| {
            let (window_width, window_height) = (win.w, win.h);

            let id = win.base_layer.id().unwrap();
            let id: usize = id.0.into();
            let dimension = binpack2d::Dimension::with_id(id as isize, (window_width * scale_factor) as i32, (window_height * scale_factor) as i32, 20);
            items_to_place.push(dimension);
        });
        bin.clear();
        (inserted, rejected) = bin.insert_list(&items_to_place);
        tries += 1;
    }

    bin
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