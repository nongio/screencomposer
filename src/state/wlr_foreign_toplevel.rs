/// Handler for wlr-foreign-toplevel-management-unstable-v1 protocol
///
/// This implements the older wlroots protocol for taskbars and window management.
/// Used by rofi, waybar, and other wlroots-based tools.
use std::sync::{Arc, Mutex};

use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use wayland_protocols_wlr::foreign_toplevel::v1::server::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

use crate::state::{Backend, ScreenComposer};

/// Global state for wlr foreign toplevel management
pub struct WlrForeignToplevelManagerState {
    instances: Vec<ZwlrForeignToplevelManagerV1>,
}

impl WlrForeignToplevelManagerState {
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<ZwlrForeignToplevelManagerV1, ()>
            + Dispatch<ZwlrForeignToplevelManagerV1, ()>
            + 'static,
    {
        display.create_global::<D, ZwlrForeignToplevelManagerV1, ()>(3, ());

        Self {
            instances: Vec::new(),
        }
    }

    #[allow(private_bounds)]
    pub fn new_toplevel<D>(
        &mut self,
        dh: &DisplayHandle,
        app_id: &str,
        title: &str,
    ) -> WlrForeignToplevelHandle
    where
        D: Dispatch<ZwlrForeignToplevelHandleV1, Arc<Mutex<WlrToplevelData>>> + 'static,
    {
        let handle_data = Arc::new(Mutex::new(WlrToplevelData {
            app_id: app_id.to_string(),
            title: title.to_string(),
            resources: Vec::new(),
        }));

        // Send toplevel to all manager instances
        for manager in &self.instances {
            if let Some(client) = manager.client() {
                let handle = client
                    .create_resource::<ZwlrForeignToplevelHandleV1, _, D>(
                        dh,
                        manager.version(),
                        handle_data.clone(),
                    )
                    .ok();

                if let Some(handle) = handle {
                    manager.toplevel(&handle);

                    // Send initial state
                    handle.app_id(app_id.to_string());
                    handle.title(title.to_string());
                    handle.done();

                    handle_data.lock().unwrap().resources.push(handle);
                }
            }
        }

        WlrForeignToplevelHandle { data: handle_data }
    }

    fn register_manager(&mut self, manager: ZwlrForeignToplevelManagerV1) {
        self.instances.push(manager);
    }

    fn unregister_manager(&mut self, manager: &ZwlrForeignToplevelManagerV1) {
        self.instances.retain(|m| m.id() != manager.id());
    }
}

/// Data associated with a wlr foreign toplevel handle
#[derive(Debug)]
struct WlrToplevelData {
    app_id: String,
    title: String,
    resources: Vec<ZwlrForeignToplevelHandleV1>,
}

/// Handle for a wlr foreign toplevel
#[derive(Debug, Clone)]
pub struct WlrForeignToplevelHandle {
    data: Arc<Mutex<WlrToplevelData>>,
}

impl WlrForeignToplevelHandle {
    pub fn send_title(&self, title: String) {
        let mut data = self.data.lock().unwrap();
        if data.title != title {
            data.title = title.clone();
            for resource in &data.resources {
                resource.title(title.clone());
                resource.done();
            }
        }
    }

    pub fn send_app_id(&self, app_id: String) {
        let mut data = self.data.lock().unwrap();
        if data.app_id != app_id {
            data.app_id = app_id.clone();
            for resource in &data.resources {
                resource.app_id(app_id.clone());
                resource.done();
            }
        }
    }

    pub fn send_closed(&self) {
        let data = self.data.lock().unwrap();
        for resource in &data.resources {
            resource.closed();
        }
    }

    pub fn title(&self) -> String {
        self.data.lock().unwrap().title.clone()
    }

    pub fn app_id(&self) -> String {
        self.data.lock().unwrap().app_id.clone()
    }
}

// Implement GlobalDispatch for manager
impl<BackendData: Backend>
    GlobalDispatch<ZwlrForeignToplevelManagerV1, (), ScreenComposer<BackendData>>
    for ScreenComposer<BackendData>
{
    fn bind(
        state: &mut ScreenComposer<BackendData>,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, ScreenComposer<BackendData>>,
    ) {
        let manager = data_init.init(resource, ());
        state
            .wlr_foreign_toplevel_state
            .register_manager(manager.clone());

        // Send all existing toplevels to this new manager
        for handles in state.foreign_toplevels.values() {
            if let Some(wlr_handle) = &handles.wlr {
                // Create a new handle resource for this manager
                if let Some(client) = manager.client() {
                    let handle = client
                        .create_resource::<ZwlrForeignToplevelHandleV1, _, ScreenComposer<BackendData>>(
                            _handle,
                            manager.version(),
                            wlr_handle.data.clone(),
                        )
                        .ok();

                    if let Some(handle) = handle {
                        manager.toplevel(&handle);

                        // Send initial state
                        let data = wlr_handle.data.lock().unwrap();
                        handle.app_id(data.app_id.clone());
                        handle.title(data.title.clone());
                        handle.done();

                        // Store handle reference
                        drop(data);
                        wlr_handle.data.lock().unwrap().resources.push(handle);
                    }
                }
            }
        }
    }
}

// Implement Dispatch for manager
impl<BackendData: Backend> Dispatch<ZwlrForeignToplevelManagerV1, (), ScreenComposer<BackendData>>
    for ScreenComposer<BackendData>
{
    fn request(
        state: &mut ScreenComposer<BackendData>,
        _client: &Client,
        resource: &ZwlrForeignToplevelManagerV1,
        request: zwlr_foreign_toplevel_manager_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, ScreenComposer<BackendData>>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Request::Stop = request {
            state
                .wlr_foreign_toplevel_state
                .unregister_manager(resource);
        }
    }

    fn destroyed(
        state: &mut ScreenComposer<BackendData>,
        _client: wayland_server::backend::ClientId,
        resource: &ZwlrForeignToplevelManagerV1,
        _data: &(),
    ) {
        state
            .wlr_foreign_toplevel_state
            .unregister_manager(resource);
    }
}

// Implement Dispatch for handle
impl<BackendData: Backend>
    Dispatch<ZwlrForeignToplevelHandleV1, Arc<Mutex<WlrToplevelData>>, ScreenComposer<BackendData>>
    for ScreenComposer<BackendData>
{
    fn request(
        _state: &mut ScreenComposer<BackendData>,
        _client: &Client,
        _resource: &ZwlrForeignToplevelHandleV1,
        request: zwlr_foreign_toplevel_handle_v1::Request,
        _data: &Arc<Mutex<WlrToplevelData>>,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, ScreenComposer<BackendData>>,
    ) {
        match request {
            zwlr_foreign_toplevel_handle_v1::Request::SetMaximized => {
                // TODO: implement maximize
                tracing::debug!("wlr foreign toplevel: set_maximized requested");
            }
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMaximized => {
                // TODO: implement unmaximize
                tracing::debug!("wlr foreign toplevel: unset_maximized requested");
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetMinimized => {
                // TODO: implement minimize
                tracing::debug!("wlr foreign toplevel: set_minimized requested");
            }
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMinimized => {
                // TODO: implement unminimize
                tracing::debug!("wlr foreign toplevel: unset_minimized requested");
            }
            zwlr_foreign_toplevel_handle_v1::Request::Activate { seat: _seat } => {
                // TODO: implement activate/focus
                tracing::debug!("wlr foreign toplevel: activate requested");
            }
            zwlr_foreign_toplevel_handle_v1::Request::Close => {
                // TODO: implement close
                tracing::debug!("wlr foreign toplevel: close requested");
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetRectangle {
                surface: _surface,
                x: _x,
                y: _y,
                width: _width,
                height: _height,
            } => {
                // TODO: implement set_rectangle (for minimize animation)
                tracing::debug!("wlr foreign toplevel: set_rectangle requested");
            }
            zwlr_foreign_toplevel_handle_v1::Request::Destroy => {
                // Handle is being destroyed by client
            }
            zwlr_foreign_toplevel_handle_v1::Request::SetFullscreen { output: _output } => {
                // TODO: implement fullscreen
                tracing::debug!("wlr foreign toplevel: set_fullscreen requested");
            }
            zwlr_foreign_toplevel_handle_v1::Request::UnsetFullscreen => {
                // TODO: implement unfullscreen
                tracing::debug!("wlr foreign toplevel: unset_fullscreen requested");
            }
            _ => {}
        }
    }
}
