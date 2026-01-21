use smithay::{
    delegate_foreign_toplevel_list,
    wayland::foreign_toplevel_list::{ForeignToplevelListHandler, ForeignToplevelListState},
};

use super::{Backend, Otto};

impl<BackendData: Backend> ForeignToplevelListHandler for Otto<BackendData> {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.foreign_toplevel_list_state
    }
}

delegate_foreign_toplevel_list!(@<BackendData: Backend + 'static> Otto<BackendData>);
