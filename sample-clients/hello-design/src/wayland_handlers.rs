use smithay_client_toolkit::{
    delegate_compositor, delegate_output, delegate_registry, delegate_seat, delegate_shm,
    delegate_xdg_shell, delegate_xdg_window, delegate_xdg_popup,
};

use crate::AppData;

// Delegate macros
delegate_compositor!(AppData);
delegate_output!(AppData);
delegate_shm!(AppData);
delegate_seat!(AppData);
delegate_xdg_shell!(AppData);
delegate_xdg_window!(AppData);
delegate_xdg_popup!(AppData);
delegate_registry!(AppData);

