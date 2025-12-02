use anyhow::Result;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, EventControllerMotion, EventSequenceState, GestureClick, Label,
    PopoverMenu, PropagationPhase,
};

fn build_menu() -> gio::Menu {
    let root = gio::Menu::new();
    let submenu_lvl2 = gio::Menu::new();
    // Add a little top padding via a spacer item.
    submenu_lvl2.append(Some("  "), None);
    submenu_lvl2.append(Some("First action"), Some("app.first"));
    submenu_lvl2.append(Some("Second action"), Some("app.second"));
    submenu_lvl2.append(Some("Disabled action"), Some("app.disabled"));
    // Bottom spacer for breathing room.
    submenu_lvl2.append(Some("  "), None);

    let submenu_lvl1 = gio::Menu::new();
    submenu_lvl1.append_submenu(Some("Level 2 submenu"), &submenu_lvl2);

    let submenu_section = gio::Menu::new();
    submenu_section.append_submenu(Some("Level 1 submenu"), &submenu_lvl1);

    root.append_section(None, &submenu_section);
    root
}

fn main() -> Result<()> {
    let app = Application::builder()
        .application_id("com.screencomposer.submenu_gtk4")
        .build();

    app.connect_startup(|app| {
        let mk_action = |name: &str| {
            let action = gio::SimpleAction::new(name, None);
            let name_owned = name.to_string();
            action.connect_activate(move |_, _| println!("Activated {}", name_owned));
            action
        };
        app.add_action(&mk_action("first"));
        app.add_action(&mk_action("second"));

        let disabled = gio::SimpleAction::new("disabled", None);
        disabled.set_enabled(false);
        app.add_action(&disabled);
    });

    app.connect_activate(|app| {
        let menu_model = build_menu();
        let popover = PopoverMenu::from_model(Some(&menu_model));
        popover.set_size_request(220, 220);

        let label = Label::builder()
            .label("Right click anywhere to open the menu with a submenu.")
            .wrap(true)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .build();

        let click = GestureClick::builder()
            .button(gdk::ffi::GDK_BUTTON_SECONDARY as u32)
            .propagation_phase(PropagationPhase::Capture)
            .build();
        click.connect_pressed(glib::clone!(@strong popover => move |gesture, _, x, y| {
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
            gesture.set_state(EventSequenceState::Claimed);
        }));

        let window = ApplicationWindow::builder()
            .application(app)
            .title("Submenu GTK4 demo")
            .default_width(480)
            .default_height(260)
            .child(&label)
            .build();

        let motion = EventControllerMotion::new();
        let pop_motion = EventControllerMotion::new();

        popover.set_parent(&window);
        popover.add_controller(pop_motion);
        window.add_controller(motion);
        window.add_controller(click);
        window.present();
    });

    app.run();
    Ok(())
}
