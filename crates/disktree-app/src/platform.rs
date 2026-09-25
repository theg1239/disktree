//! Desktop integration around the same GPUI/Omarchy components.
#![allow(
    clippy::derive_partial_eq_without_eq,
    reason = "GPUI's actions! macro derives PartialEq for its generated types"
)]
use gpui_kit::{App, Window};

gpui_kit::actions!(
    disktree,
    [
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        OpenFolder,
        Reveal,
        Rescan,
        ScanDisk,
        CloseWindow,
        FullDiskAccess
    ]
);

pub fn init(cx: &mut App) {
    gpui_omarchy::init(cx);
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    #[cfg(target_os = "macos")]
    {
        use gpui_kit::{KeyBinding, Menu, MenuItem};
        apply_appearance(cx.window_appearance(), cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &Hide, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
        cx.on_action(|_: &FullDiskAccess, cx| {
            cx.open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles");
        });
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
            KeyBinding::new("cmd-o", OpenFolder, Some("Disktree")),
            KeyBinding::new("cmd-r", Rescan, Some("Disktree")),
            KeyBinding::new("cmd-shift-r", Reveal, Some("Disktree")),
            KeyBinding::new("cmd-w", CloseWindow, Some("Disktree")),
        ]);
        cx.set_menus([
            Menu::new("Disktree").items([
                MenuItem::action("Hide Disktree", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Disktree", Quit),
            ]),
            Menu::new("File").items([
                MenuItem::action("Open Folder…", OpenFolder),
                MenuItem::action("Reveal in Finder", Reveal),
                MenuItem::separator(),
                MenuItem::action("Scan Again", Rescan),
                MenuItem::action("Scan the Whole Volume", ScanDisk),
                MenuItem::separator(),
                MenuItem::action("Full Disk Access Settings…", FullDiskAccess),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ]),
        ]);
    }
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(
        clippy::missing_const_for_fn,
        reason = "the macOS variant installs a non-const native appearance observer"
    )
)]
pub fn init_window(window: &Window, _cx: &mut App) {
    #[cfg(target_os = "macos")]
    window
        .observe_window_appearance(|window, cx| {
            apply_appearance(window.appearance(), cx);
        })
        .detach();
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

#[cfg(target_os = "macos")]
fn apply_appearance(appearance: gpui_kit::WindowAppearance, cx: &mut App) {
    use gpui_kit::base::ThemeAppearance;
    use gpui_kit::{WindowAppearance, rgb};
    use gpui_omarchy::Theme;
    let dark = matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    );
    let mut theme = if dark {
        Theme::tokyo_night()
    } else {
        Theme::flexoki_light()
    };
    theme.name = "macOS".into();
    theme.font = ".SystemUIFont".into();
    theme.appearance = if dark {
        ThemeAppearance::Dark
    } else {
        ThemeAppearance::Light
    };
    let colors = if dark {
        [
            0x1c_1c_1e, 0x24_24_26, 0x15_15_17, 0xd7_d7_dc, 0x98_98_9f,
            0xf5_f5_f7, 0x0a_84_ff, 0xff_ff_ff, 0x34_34_38, 0x48_48_4d,
            0xff_69_61, 0xe9_b4_5a, 0x55_bd_70,
        ]
    } else {
        [
            0xf5_f5_f7, 0xff_ff_ff, 0xea_ea_ed, 0x30_30_36, 0x64_64_6c,
            0x18_18_1b, 0x00_6c_db, 0xff_ff_ff, 0xdb_e9_fa, 0xc6_c6_ce,
            0xc6_2e_26, 0x8e_5e_0d, 0x24_7d_3b,
        ]
    };
    let [
        background,
        surface,
        inset,
        foreground,
        secondary,
        bright,
        accent,
        on_accent,
        selection,
        border,
        danger,
        warning,
        success,
    ] = colors.map(|c| rgb(c).into());
    theme.background = background;
    theme.surface = surface;
    theme.inset = inset;
    theme.foreground = foreground;
    theme.secondary = secondary;
    theme.bright = bright;
    theme.accent = accent;
    theme.on_accent = on_accent;
    theme.selection = selection;
    theme.border = border;
    theme.danger = danger;
    theme.warning = warning;
    theme.success = success;
    theme.apply(cx);
}
