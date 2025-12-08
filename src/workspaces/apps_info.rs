use std::sync::Arc;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use freedesktop_desktop_entry::DesktopEntry;

use lay_rs::skia;

use crate::{config::Config, utils::image_from_path};

#[derive(Clone)]
pub struct Application {
    pub identifier: String,
    pub match_id: String,
    pub icon_path: Option<String>,
    pub icon: Option<skia::Image>,
    pub picture: Option<skia::Picture>,
    pub override_name: Option<String>,
    pub desktop_file_id: Option<String>,
    desktop_entry: DesktopEntry,
}

impl Application {
    pub fn desktop_name(&self) -> Option<String> {
        if let Some(name) = &self.override_name {
            return Some(name.clone());
        }
        Config::with(|c| {
            self.desktop_entry
                .name(&c.locales)
                .map(|name| name.to_string())
        })
    }
    pub fn command(&self, extra_args: &[String]) -> Option<(String, Vec<String>)> {
        let exec = self.desktop_entry.exec()?;
        let mut parts = shell_words::split(exec).ok()?;
        if parts.is_empty() {
            return None;
        }
        let cmd = parts.remove(0);
        let mut args: Vec<String> = parts
            .into_iter()
            .filter_map(|arg| {
                if arg.starts_with('%') {
                    None
                } else {
                    Some(arg)
                }
            })
            .collect();
        args.extend(extra_args.iter().cloned());
        Some((cmd, args))
    }
}

impl Hash for Application {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.match_id.hash(state);
        self.icon_path.hash(state);
        self.override_name.hash(state);

        if let Some(i) = self.icon.as_ref() {
            i.unique_id().hash(state)
        }
    }
}

impl PartialEq for Application {
    fn eq(&self, other: &Self) -> bool {
        self.match_id == other.match_id
    }
}
impl Eq for Application {}

impl std::fmt::Debug for Application {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Application")
            .field("identifier", &self.identifier)
            .field("match_id", &self.match_id)
            .field("desktop_file_id", &self.desktop_file_id)
            .field("icon_path", &self.icon_path)
            .field("icon", &self.icon.is_some())
            .field("override_name", &self.override_name)
            .finish()
    }
}

type AppsInfoStorage = HashMap<String, Application>;

fn applications_info() -> &'static Arc<tokio::sync::RwLock<AppsInfoStorage>> {
    static INSTANCE: std::sync::OnceLock<Arc<tokio::sync::RwLock<HashMap<String, Application>>>> =
        std::sync::OnceLock::new();

    INSTANCE.get_or_init(|| Arc::new(tokio::sync::RwLock::new(HashMap::new())))
}

pub struct ApplicationsInfo;

impl ApplicationsInfo {
    pub async fn get_app_info_by_id(app_id: impl Into<String>) -> Option<Application> {
        let app_id = app_id.into();
        let mut applications = applications_info().write().await;
        let mut app = { applications.get(&app_id).cloned() };
        if app.is_none() {
            if let Some(new_app) = ApplicationsInfo::load_app_info(&app_id).await {
                applications.insert(app_id.clone(), new_app.clone());
                app = Some(new_app);
            }
        }

        app
    }

    async fn get_desktop_entry(app_id: &str) -> Option<DesktopEntry> {
        let entry_path =
            freedesktop_desktop_entry::Iter::new(freedesktop_desktop_entry::default_paths())
                .find(|path| path.to_string_lossy().contains(app_id));

        entry_path.as_ref()?;
        let entry_path = entry_path.unwrap();
        let locales = &["en"];
        DesktopEntry::from_path(entry_path, Some(locales)).ok()
    }

    async fn load_app_info(app_id: impl Into<String>) -> Option<Application> {
        let app_id = app_id.into();

        tracing::info!("load_app_info: {}", app_id);

        let desktop_entry = ApplicationsInfo::get_desktop_entry(&app_id).await;

        if let Some(desktop_entry) = desktop_entry {
            let match_id = desktop_entry.id().to_string();
            let identifier = if app_id.ends_with(".desktop") {
                match_id.clone()
            } else {
                app_id.clone()
            };
            let icon_path = desktop_entry
                .icon()
                .map(|icon| icon.to_string())
                .and_then(|icon_name| xdgkit::icon_finder::find_icon(icon_name, 512, 1))
                .map(|icon| icon.to_str().unwrap().to_string());

            let icon = icon_path.as_ref().and_then(|icon_path| {
                // let icon_path = "/home/riccardo/.local/share/icons/WhiteSur/apps/scalable/org.gnome.gedit.svg";
                image_from_path(icon_path, (512, 512))
            });
            if icon.is_none() {
                tracing::warn!("icon loading failed: {:?}", icon_path);
            } else {
                tracing::info!("icon loaded: {:?}", icon_path);
            }
            // let picture = icon_path
            //     .as_ref()
            //     .and_then(|icon_path| {
            //         let path = std::path::Path::new(icon_path);
            //         if path.extension().and_then(std::ffi::OsStr::to_str) == Some("svg") {
            //             if let Ok(svg) = svg_dom(icon_path, (100, 100)) {
            //                 let mut rec = skia::PictureRecorder::new();
            //                 let canvas = rec.begin_recording(skia::Rect::from_iwh(512, 512), None);
            //                 svg.render(&canvas);
            //                 // let paint = skia::Paint::new(skia::Color4f::new(1.0, 0.0, 0.0, 1.0), None);
            //                 // canvas.draw_circle((50.0, 50.0), 50.0, &paint);
            //                 return rec.finish_recording_as_picture(None)
            //             }
            //         }
            //         None
            //     });

            let desktop_file_id = desktop_entry
                .path
                .file_stem()
                .and_then(|os| os.to_str())
                .map(|s| s.to_string());

            let app = Application {
                identifier,
                match_id,
                icon_path,
                icon,
                picture: None,
                override_name: None,
                desktop_file_id,
                desktop_entry,
            };

            return Some(app);
        }
        None
    }
}

#[tokio::test]
async fn async_load_app_information() {
    let app_info = ApplicationsInfo::get_app_info_by_id("org.kde.dolphin")
        .await
        .unwrap();

    assert_eq!(app_info.identifier, "org.kde.dolphin");
    assert!(app_info.desktop_name().is_some());
    assert!(app_info.icon_path.is_some());
    println!("{:?}", app_info);
}
