//! The shell's own state, without a screen: panels opening, switching and closing, and
//! notifications coming, going and showing as banners. (DMS's rule: test services and state,
//! never layout. How things look is the screen checks' job, `screens.rs`.)

use super::*;
use crate::panels::action::Note;
use crate::services::notifications::{Event, Notification};

const SCREEN_W: f32 = 1920.0;
const SCREEN_H: f32 = 1080.0;

/// A screen kestrel was told about, as the compositor would tell it.
fn output(n: u32, name: &str) -> Output {
    let id = OutputId::for_tests(n);
    Output {
        id,
        info: iced::OutputInfo {
            id,
            name: name.into(),
            description: None,
            scale_factor: 1,
            logical_size: Some((SCREEN_W as i32, SCREEN_H as i32)),
            make: String::new(),
            model: String::new(),
        },
        name: name.into(),
        size: Size::new(SCREEN_W, SCREEN_H),
        overlay: SurfaceId::unique(),
        bar: SurfaceId::unique(),
        bar_keyboard: false,
        surfaces: Vec::new(),
        reserves: Vec::new(),
        plan: None,
        pill_x: Spring::new(0.0),
        pill_r: Spring::new(0.0),
        island_w: Spring::new(0.0),
    }
}

/// The shell as started (the defaults: tests never read the user's config), on two screens, with
/// nothing in the notification list and alert sounds off (a test never plays one).
fn app() -> App {
    let (mut app, _) = boot();
    app.config = Config::default();
    app.config.notifications.sound = false;
    app.config.sound.screenshot = false;
    app.dnd = false;
    app.notifications.clear();
    app.outputs = vec![output(1, "DP-1"), output(2, "DP-2")];
    app.hypr.focused_monitor = "DP-1".into();
    app
}

fn screen(n: u32) -> OutputId {
    OutputId::for_tests(n)
}

/// A bar button's place at `x`.
fn button(x: f32) -> Rectangle {
    Rectangle {
        x,
        y: 0.0,
        width: 30.0,
        height: tok::BAR,
    }
}

fn open_kind(app: &App) -> Option<(PanelKind, OutputId)> {
    app.panel
        .as_ref()
        .filter(|p| p.open)
        .map(|p| (p.kind, p.output))
}

fn note(id: u32, app_name: &str) -> Notification {
    Notification {
        id,
        app: app_name.into(),
        app_icon: String::new(),
        summary: format!("Message {id}"),
        body: String::new(),
        actions: Vec::new(),
        image: None,
        urgency: 1,
        timeout: -1,
        time: std::time::SystemTime::now(),
        sound: Default::default(),
        replaced: false,
    }
}

fn add(app: &mut App, n: Notification) {
    let _ = app.notify(Event::Added(Box::new(n)));
}

fn ids(app: &App) -> Vec<u32> {
    app.notifications.iter().map(|n| n.id).collect()
}

fn toast_ids(app: &App) -> Vec<u32> {
    app.toasts.iter().map(|t| t.id).collect()
}

/// An "Open with" question, as the portal asks it.
fn ask(id: u64) -> crate::services::appchooser::Ask {
    crate::services::appchooser::Ask {
        id,
        app_id: String::new(),
        choices: vec!["org.gnome.Loupe".into()],
        last: None,
        what: "a.png".into(),
        content_type: "image/png".into(),
    }
}

// --- panels ------------------------------------------------------------------------------------

#[test]
fn a_toggle_opens_the_panel_under_its_button() {
    let mut app = app();
    let _ = app.toggle(PanelKind::Calendar, screen(1), button(900.0));
    assert_eq!(open_kind(&app), Some((PanelKind::Calendar, screen(1))));
}

#[test]
fn the_same_toggle_again_closes_it() {
    let mut app = app();
    let _ = app.toggle(PanelKind::Calendar, screen(1), button(900.0));
    let _ = app.toggle(PanelKind::Calendar, screen(1), button(900.0));
    assert!(app.panel.is_none());
}

#[test]
fn another_panel_replaces_the_open_one() {
    let mut app = app();
    let _ = app.toggle(PanelKind::Calendar, screen(1), button(900.0));
    let _ = app.toggle(PanelKind::Power, screen(1), button(1800.0));
    assert_eq!(open_kind(&app), Some((PanelKind::Power, screen(1))));
}

#[test]
fn the_same_panel_from_another_screen_moves_there() {
    let mut app = app();
    let _ = app.toggle(PanelKind::Calendar, screen(1), button(900.0));
    let _ = app.toggle(PanelKind::Calendar, screen(2), button(900.0));
    assert_eq!(open_kind(&app), Some((PanelKind::Calendar, screen(2))));
}

#[test]
fn a_screen_kestrel_does_not_know_opens_nothing() {
    let mut app = app();
    let _ = app.toggle(PanelKind::Calendar, screen(9), button(900.0));
    assert!(app.panel.is_none());
}

#[test]
fn a_question_on_screen_is_answered_when_another_panel_takes_its_place() {
    let mut app = app();
    crate::panels::open_with::begin(&mut app.panels.open_with, ask(7));
    let _ = app.open_panel(PanelKind::OpenWith, screen(1), button(900.0));
    assert!(app.panels.open_with.ask.is_some());
    let _ = app.toggle(PanelKind::Calendar, screen(1), button(900.0));
    assert!(
        app.panels.open_with.ask.is_none(),
        "the app that asked hears \"cancelled\""
    );
    assert_eq!(open_kind(&app), Some((PanelKind::Calendar, screen(1))));
}

#[test]
fn closing_answers_the_question_too() {
    let mut app = app();
    crate::panels::open_with::begin(&mut app.panels.open_with, ask(8));
    let _ = app.open_panel(PanelKind::OpenWith, screen(1), button(900.0));
    let _ = app.close();
    assert!(app.panels.open_with.ask.is_none());
    assert!(app.panel.is_none());
}

#[test]
fn a_question_is_kept_while_its_own_panel_moves_screens() {
    let mut app = app();
    crate::panels::open_with::begin(&mut app.panels.open_with, ask(9));
    let _ = app.open_panel(PanelKind::OpenWith, screen(1), button(900.0));
    let _ = app.open_panel(PanelKind::OpenWith, screen(2), button(900.0));
    assert!(app.panels.open_with.ask.is_some());
}

#[test]
fn no_notifications_says_so_in_the_island_instead_of_an_empty_list() {
    let mut app = app();
    let _ = app.toggle(PanelKind::Notifications, screen(1), button(1800.0));
    assert!(app.panel.is_none());
    let osd = app.osd.as_ref().expect("the island says it");
    assert_eq!(osd.text.as_deref(), Some("No notifications"));
    assert_eq!(osd.sub, None, "no empty second line");
}

#[test]
fn opening_the_list_marks_everything_read_and_takes_the_banners() {
    let mut app = app();
    add(&mut app, note(1, "Mail"));
    add(&mut app, note(2, "Mail"));
    assert!(app.unread);
    assert_eq!(app.unseen, 2);
    assert_eq!(app.toasts.len(), 2);
    let _ = app.toggle(PanelKind::Notifications, screen(1), button(1800.0));
    assert_eq!(open_kind(&app), Some((PanelKind::Notifications, screen(1))));
    assert!(!app.unread);
    assert_eq!(app.unseen, 0);
    assert!(app.toasts.is_empty());
    // And the same button closes it again.
    let _ = app.toggle(PanelKind::Notifications, screen(1), button(1800.0));
    assert!(app.panel.is_none());
}

#[test]
fn the_list_closes_with_its_last_notification() {
    let mut app = app();
    add(&mut app, note(1, "Mail"));
    add(&mut app, note(2, "Chat"));
    let _ = app.toggle(PanelKind::Notifications, screen(1), button(1800.0));
    let _ = app.note(Note::Dismiss(1));
    assert!(open_kind(&app).is_some(), "one is left");
    // Through the shell's update, as the panel's ✕ arrives: the empty list is closed there,
    // for every way the last notification can go (an app closing it too).
    let _ = super::update(
        &mut app,
        Message::Panel(crate::panels::Message::Notifications(
            crate::panels::notifications::Message::Dismiss(2),
        )),
    );
    assert!(
        app.panel.is_none(),
        "closed in the same update, never drawn empty"
    );
}

#[test]
fn a_panel_waiting_for_its_reading_does_not_open_yet() {
    let mut app = app();
    let _ = app.toggle(PanelKind::Monitor, screen(1), button(900.0));
    assert!(app.panel.is_none(), "it opens when its first reading is in");
}

#[test]
fn panels_hang_under_their_button_but_never_off_the_screen() {
    let (w, screen_w, edge) = (400.0, 1920.0, 4.0);
    assert_eq!(place(960.0, w, screen_w, edge), 760.0, "centred");
    assert_eq!(
        place(10.0, w, screen_w, edge),
        edge,
        "held at the left edge"
    );
    assert_eq!(
        place(1910.0, w, screen_w, edge),
        screen_w - edge - w,
        "held at the right edge"
    );
    // Close to an edge (not over it) it joins the edge too: no sliver of wallpaper.
    let near = edge + tok::JOIN + 24.0 + w / 2.0 - 1.0;
    assert_eq!(place(near, w, screen_w, edge), edge);
}

#[test]
fn level_popups_stay_away_while_a_panel_is_open() {
    let mut app = app();
    app.show_osd(Icon::Volume2, 0.5);
    assert!(app.osd.is_some());
    app.hide_osd();
    let _ = app.toggle(PanelKind::Calendar, screen(1), button(900.0));
    app.show_osd(Icon::Volume2, 0.5);
    assert!(app.osd.is_none(), "the open panel shows the level itself");
}

// --- notifications -----------------------------------------------------------------------------

#[test]
fn a_new_notification_goes_on_top_with_a_banner() {
    let mut app = app();
    add(&mut app, note(1, "Mail"));
    add(&mut app, note(2, "Chat"));
    assert_eq!(ids(&app), [1, 2], "oldest first: the newest is the last");
    assert_eq!(toast_ids(&app), [1, 2]);
    assert_eq!(app.toast_last, Some(2));
}

#[test]
fn an_update_changes_a_notification_where_it_is() {
    let mut app = app();
    add(&mut app, note(1, "Downloads"));
    add(&mut app, note(2, "Mail"));
    let mut n = note(1, "Downloads");
    n.body = "80%".into();
    n.replaced = true;
    add(&mut app, n);
    assert_eq!(ids(&app), [1, 2], "kept in its place");
    assert_eq!(app.notifications[0].body, "80%");
}

#[test]
fn the_same_id_sent_again_as_new_moves_to_the_top() {
    let mut app = app();
    add(&mut app, note(1, "Mail"));
    add(&mut app, note(2, "Mail"));
    add(&mut app, note(1, "Mail"));
    assert_eq!(ids(&app), [2, 1]);
    assert_eq!(toast_ids(&app), [2, 1], "one banner per notification");
}

#[test]
fn the_list_keeps_the_newest_fifty() {
    let mut app = app();
    for i in 1..=55 {
        add(&mut app, note(i, "Spam"));
    }
    assert_eq!(app.notifications.len(), 50);
    assert_eq!(app.notifications[0].id, 6);
    assert_eq!(app.notifications[49].id, 55);
    assert_eq!(app.toasts.len(), 3, "three banners at most");
    assert_eq!(toast_ids(&app), [53, 54, 55]);
}

#[test]
fn a_muted_app_is_dropped() {
    let mut app = app();
    app.config.notifications.muted = vec!["Spam".into()];
    add(&mut app, note(1, "Spam"));
    assert!(app.notifications.is_empty());
    assert!(app.toasts.is_empty());
    assert!(!app.unread);
}

#[test]
fn a_quiet_app_is_listed_without_a_banner() {
    let mut app = app();
    app.config.notifications.quiet = vec!["Steam".into()];
    add(&mut app, note(1, "Steam"));
    assert_eq!(ids(&app), [1]);
    assert!(app.toasts.is_empty());
    assert!(app.unread);
    // Unless it is urgent.
    let mut n = note(2, "Steam");
    n.urgency = 2;
    add(&mut app, n);
    assert_eq!(toast_ids(&app), [2]);
}

#[test]
fn do_not_disturb_holds_banners_but_lets_urgent_ones_through() {
    let mut app = app();
    app.dnd = true;
    add(&mut app, note(1, "Mail"));
    assert_eq!(ids(&app), [1]);
    assert!(app.toasts.is_empty());
    let mut call = note(2, "Phone");
    call.urgency = 2;
    add(&mut app, call.clone());
    assert_eq!(toast_ids(&app), [2]);
    // Unless the user said urgent ones wait too.
    app.toasts.clear();
    app.config.notifications.urgent_in_dnd = false;
    call.id = 3;
    add(&mut app, call);
    assert!(app.toasts.is_empty());
}

#[test]
fn a_fullscreen_app_is_not_interrupted() {
    let mut app = app();
    app.hypr.fullscreen = vec!["DP-1".into()];
    add(&mut app, note(1, "Mail"));
    assert!(app.toasts.is_empty());
    assert!(app.unread, "it waits in the list");
    let mut n = note(2, "Phone");
    n.urgency = 2;
    add(&mut app, n);
    assert_eq!(toast_ids(&app), [2]);
}

#[test]
fn with_the_list_open_nothing_is_unread() {
    let mut app = app();
    add(&mut app, note(1, "Mail"));
    let _ = app.toggle(PanelKind::Notifications, screen(1), button(1800.0));
    add(&mut app, note(2, "Mail"));
    assert_eq!(ids(&app), [1, 2]);
    assert!(!app.unread);
    assert_eq!(app.unseen, 0);
    assert!(app.toasts.is_empty(), "the list shows it, not a banner");
}

#[test]
fn banners_last_as_the_app_asks_within_limits() {
    let mut app = app();
    let secs = |app: &App, id| {
        app.toasts
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.secs.as_secs())
    };
    let mut n = note(1, "A");
    n.timeout = 10_000;
    add(&mut app, n);
    let mut n = note(2, "A");
    n.timeout = 500;
    add(&mut app, n);
    let mut n = note(3, "A");
    n.timeout = 600_000;
    add(&mut app, n);
    assert_eq!(secs(&app, 1), Some(10));
    assert_eq!(secs(&app, 2), Some(2), "never too short to read");
    assert_eq!(secs(&app, 3), Some(30), "never stays for good");
    app.toasts.clear();
    app.config.notifications.banner = 7;
    add(&mut app, note(4, "A"));
    assert_eq!(
        secs(&app, 4),
        Some(7),
        "the app did not say: the user's setting"
    );
    app.config.notifications.banner = 0;
    add(&mut app, note(5, "A"));
    assert_eq!(secs(&app, 5), Some(2));
    let mut n = note(6, "A");
    n.urgency = 2;
    add(&mut app, n);
    assert_eq!(secs(&app, 6), Some(3600), "urgent stays until dealt with");
}

#[test]
fn a_notification_closed_by_its_app_goes_with_its_banner() {
    let mut app = app();
    add(&mut app, note(1, "A"));
    add(&mut app, note(2, "A"));
    let _ = app.notify(Event::Closed(1));
    assert_eq!(ids(&app), [2]);
    assert_eq!(toast_ids(&app), [2]);
    // An id that is not there: nothing.
    let _ = app.notify(Event::Closed(77));
    assert_eq!(ids(&app), [2]);
}

#[test]
fn dismissing_by_one_by_app_and_all() {
    let mut app = app();
    for (id, a) in [(1, "Mail"), (2, "Chat"), (3, "Mail"), (4, "Chat")] {
        add(&mut app, note(id, a));
    }
    let _ = app.note(Note::Dismiss(2));
    assert_eq!(ids(&app), [1, 3, 4]);
    let _ = app.note(Note::DismissApp("Mail".into()));
    assert_eq!(ids(&app), [4]);
    assert_eq!(toast_ids(&app), [4], "their banners go too");
    let _ = app.note(Note::ClearAll);
    assert!(app.notifications.is_empty());
    assert!(app.toasts.is_empty());
}

#[test]
fn pressing_a_notifications_button_takes_it_away() {
    let mut app = app();
    add(&mut app, note(1, "Mail"));
    let _ = app.note(Note::Invoke(1, "default".into()));
    assert!(app.notifications.is_empty());
    assert!(app.toasts.is_empty());
}

#[test]
fn hiding_a_banner_keeps_the_notification() {
    let mut app = app();
    add(&mut app, note(1, "Mail"));
    let _ = app.note(Note::HideBanner(1));
    assert_eq!(ids(&app), [1]);
    assert!(app.toasts.is_empty());
}

#[test]
fn banners_expire_but_not_under_the_pointer() {
    let mut app = app();
    add(&mut app, note(1, "A"));
    add(&mut app, note(2, "A"));
    let _ = app.note(Note::HoldBanner(1, true));
    let later = Instant::now() + Duration::from_secs(60);
    app.expire(later);
    assert_eq!(toast_ids(&app), [1], "the held one stays");
    assert_eq!(ids(&app), [1, 2], "expiring takes only banners");
    // Let go: its time starts again in full.
    let _ = app.note(Note::HoldBanner(1, false));
    assert_eq!(app.toast_hover, None);
    let t = &app.toasts[0];
    assert!(t.until > Instant::now() + t.secs - Duration::from_secs(1));
    app.expire(Instant::now() + Duration::from_secs(1));
    assert_eq!(toast_ids(&app), [1]);
    app.expire(later + Duration::from_secs(60));
    assert!(app.toasts.is_empty());
}

#[test]
fn letting_go_of_another_banner_keeps_the_held_one_held() {
    let mut app = app();
    add(&mut app, note(1, "A"));
    add(&mut app, note(2, "A"));
    let _ = app.note(Note::HoldBanner(1, true));
    let _ = app.note(Note::HoldBanner(2, false));
    assert_eq!(app.toast_hover, Some(1));
}

#[test]
fn kestrels_own_entries_are_listed_quietly_with_their_own_ids() {
    let mut app = app();
    app.remember(
        "camera",
        "Screenshot".into(),
        String::new(),
        None,
        Vec::new(),
    );
    app.remember(
        "camera",
        "Recording".into(),
        String::new(),
        None,
        Vec::new(),
    );
    assert_eq!(app.notifications.len(), 2);
    assert!(!app.unread);
    assert!(app.toasts.is_empty());
    let (a, b) = (app.notifications[0].id, app.notifications[1].id);
    assert_ne!(a, b);
    assert!(
        a > 0x4000_0000 && b > 0x4000_0000,
        "clear of the apps' and restored ids"
    );
    for _ in 0..60 {
        app.remember("camera", "Shot".into(), String::new(), None, Vec::new());
    }
    assert_eq!(app.notifications.len(), 50);
}

#[test]
fn copying_says_so_in_the_island() {
    use crate::services::clipboard::Copied;
    let mut app = app();
    app.copied(Copied::Text("secret".into()));
    let f = app.flash.as_ref().unwrap();
    assert_eq!(f.title, "Copied");
    assert!(f.detail.is_empty(), "the text itself is never shown");
    let image = |w, h| Copied::Image {
        width: w,
        height: h,
        thumb: (0, 0, Vec::new()),
        file: None,
    };
    app.copied(image(SCREEN_W as u32, SCREEN_H as u32));
    assert_eq!(app.flash.as_ref().unwrap().title, "Screenshot copied");
    app.copied(image(300, 200));
    assert_eq!(app.flash.as_ref().unwrap().title, "Image copied");
    // Gone when its time is up.
    app.expire(Instant::now() + Duration::from_secs(60));
    assert!(app.flash.is_none());
}

fn battery(on_battery: bool, warning: u32, percent: f32) -> power::State {
    power::State {
        has_battery: true,
        on_battery,
        percent,
        warning,
        ..power::State::default()
    }
}

/// UPower says `s`: the shell warns (or not), then keeps it as the state.
fn power_now(app: &mut App, s: power::State) {
    app.battery_warning(&s);
    app.power = s;
}

#[test]
fn a_low_battery_is_warned_once_then_critical_replaces_it() {
    let mut app = app();
    power_now(&mut app, battery(true, 1, 40.0));
    assert!(app.notifications.is_empty());
    power_now(&mut app, battery(true, 3, 10.0));
    assert_eq!(app.notifications.len(), 1);
    assert_eq!(app.notifications[0].summary, "Battery low");
    assert!(app.notifications[0].body.starts_with("10% left"));
    // Still low a minute later: not again.
    power_now(&mut app, battery(true, 3, 9.0));
    assert_eq!(app.notifications.len(), 1);
    power_now(&mut app, battery(true, 4, 4.0));
    assert_eq!(
        app.notifications.len(),
        1,
        "the critical one replaces the low one"
    );
    assert_eq!(app.notifications[0].summary, "Battery almost empty");
    assert_eq!(app.notifications[0].urgency, 2);
}

#[test]
fn the_charger_takes_the_warning_away() {
    let mut app = app();
    power_now(&mut app, battery(true, 1, 20.0));
    power_now(&mut app, battery(true, 3, 10.0));
    assert_eq!(app.notifications.len(), 1);
    power_now(&mut app, battery(false, 1, 10.0));
    assert!(app.notifications.is_empty());
    assert_eq!(app.battery_note, None);
    // Unplugged again while low: warned again.
    power_now(&mut app, battery(true, 3, 10.0));
    assert_eq!(app.notifications.len(), 1);
}

#[test]
fn no_battery_no_warning() {
    let mut app = app();
    let s = power::State {
        has_battery: false,
        on_battery: true,
        warning: 4,
        ..power::State::default()
    };
    power_now(&mut app, s);
    assert!(app.notifications.is_empty());
}

#[test]
fn bluetooth_device_icons() {
    assert!(matches!(device_icon("audio-headphones"), Icon::Headphones));
    assert!(matches!(device_icon("input-keyboard"), Icon::Keyboard));
    assert!(matches!(device_icon("smartwatch"), Icon::Watch));
    assert!(matches!(device_icon("something-new"), Icon::Bluetooth));
}
