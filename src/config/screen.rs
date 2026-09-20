use iced::advanced::image::Handle;
use iced::advanced::text::Wrapping;
use iced::alignment::Horizontal;
use iced::widget::{
    Column, button, checkbox, column, container, image, pick_list, row, rule, scrollable, space,
    text, text_input,
};
use iced::{Alignment, ContentFit, Element, Length, Padding, window};

use crate::input::Modifier;
use crate::theme;

use super::{Config, FilterAction, FilterRule, Pattern, Side, ThemeMode};

const SIDEBAR_WIDTH: f32 = 135.0;
const CONTROL_WIDTH: f32 = 160.0;
const WIDE_CONTROL_WIDTH: f32 = 240.0;
const LABEL_SIZE: f32 = 13.0;
const NOTE_SIZE: f32 = 11.0;
const TITLE_SIZE: f32 = 20.0;
const PANE_PADDING: f32 = 16.0;
const ICON_RENDER_SIZE: f32 = 96.0;

const ICON: &[u8] = include_bytes!("../../assets/icon.png");
const LICENSE: &str = "GPLv3";
const SOURCE: &str = "https://git.sr.ht/~flamarine/atray";
const ISSUES: &str = "https://todo.sr.ht/~flamarine/atray";

const MODIFIER_LABELS: [&str; 4] = ["Alt", "Control", "Shift", "Super"];
const SIDE_LABELS: [&str; 4] = ["Left", "Right", "Top", "Bottom"];
const THEME_LABELS: [&str; 3] = ["System", "Light", "Dark"];
const ACTION_LABELS: [&str; 2] = ["Allow", "Deny"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Behavior,
    Appearance,
    Advanced,
    About,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Behavior, Tab::Appearance, Tab::Advanced, Tab::About];

    fn label(self) -> &'static str {
        match self {
            Tab::Behavior => "Behavior",
            Tab::Appearance => "Appearance",
            Tab::Advanced => "Advanced",
            Tab::About => "About",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(Tab),
    MoveModifier(Modifier),
    InvertCopyAndMove(bool),
    RuleApp(usize, String),
    RuleTitle(usize, String),
    RuleAction(usize, FilterAction),
    RuleRemove(usize),
    RuleAdd,
    Side(Side),
    Theme(ThemeMode),
    CacheDir(String),
    Revert,
    Save,
    OpenLink(&'static str),
}

#[derive(Debug, Clone)]
struct RuleDraft {
    app: String,
    title: String,
    action: FilterAction,
}

impl RuleDraft {
    fn from_rule(rule: &FilterRule) -> Self {
        Self {
            app: rule
                .app
                .as_ref()
                .map(|pattern| pattern.as_str().to_owned())
                .unwrap_or_default(),
            title: rule
                .title
                .as_ref()
                .map(|pattern| pattern.as_str().to_owned())
                .unwrap_or_default(),
            action: rule.action,
        }
    }

    fn to_rule(&self, index: usize) -> Result<FilterRule, String> {
        Ok(FilterRule {
            app: pattern(&self.app).map_err(|error| format!("Rule {}: app: {error}", index + 1))?,
            title: pattern(&self.title)
                .map_err(|error| format!("Rule {}: title: {error}", index + 1))?,
            action: self.action,
        })
    }
}

fn pattern(value: &str) -> Result<Option<Pattern>, String> {
    let value = value.trim();

    if value.is_empty() {
        return Ok(None);
    }

    Pattern::parse(value)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub struct Screen {
    tab: Tab,
    move_modifier: Modifier,
    invert_copy_and_move: bool,
    rules: Vec<RuleDraft>,
    side: Side,
    theme: ThemeMode,
    cache_dir: String,
    error: Option<String>,
    icon: Option<Handle>,
}

impl Screen {
    pub fn from_config(config: &Config) -> Self {
        Self {
            tab: Tab::Behavior,
            move_modifier: config.behavior.move_modifier,
            invert_copy_and_move: config.behavior.invert_copy_and_move,
            rules: config
                .behavior
                .filter
                .iter()
                .map(RuleDraft::from_rule)
                .collect(),
            side: config.appearance.side,
            theme: config.appearance.theme,
            cache_dir: config.advanced.cache_dir.clone(),
            error: None,
            icon: icon_handle(),
        }
    }

    pub fn apply(&self, config: &mut Config) -> Result<(), String> {
        let mut filter = Vec::with_capacity(self.rules.len());

        for (index, rule) in self.rules.iter().enumerate() {
            filter.push(rule.to_rule(index)?);
        }

        config.behavior.move_modifier = self.move_modifier;
        config.behavior.invert_copy_and_move = self.invert_copy_and_move;
        config.behavior.filter = filter;
        config.appearance.side = self.side;
        config.appearance.theme = self.theme;
        config.advanced.cache_dir = self.cache_dir.trim().to_owned();

        Ok(())
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Tab(tab) => self.tab = tab,
            Message::MoveModifier(modifier) => self.move_modifier = modifier,
            Message::InvertCopyAndMove(value) => self.invert_copy_and_move = value,
            Message::RuleApp(index, value) => {
                if let Some(rule) = self.rules.get_mut(index) {
                    rule.app = value;
                }
            }
            Message::RuleTitle(index, value) => {
                if let Some(rule) = self.rules.get_mut(index) {
                    rule.title = value;
                }
            }
            Message::RuleAction(index, action) => {
                if let Some(rule) = self.rules.get_mut(index) {
                    rule.action = action;
                }
            }
            Message::RuleRemove(index) => {
                if index < self.rules.len() {
                    self.rules.remove(index);
                }
            }
            Message::RuleAdd => self.rules.push(RuleDraft {
                app: String::new(),
                title: String::new(),
                action: FilterAction::default(),
            }),
            Message::Side(side) => self.side = side,
            Message::Theme(theme) => self.theme = theme,
            Message::CacheDir(value) => self.cache_dir = value,
            Message::Revert | Message::Save => {}
            Message::OpenLink(url) => {
                let _ = open::that(url);
            }
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn view(&self, colors: theme::Colors, radius: f32) -> Element<'_, Message> {
        let sidebar = container(self.sidebar(radius))
            .style(theme::settings_sidebar)
            .width(SIDEBAR_WIDTH)
            .height(Length::Fill);

        let pane = scrollable(
            container(self.pane(colors, radius))
                .width(Length::Fill)
                .padding(Padding {
                    top: crate::platform::titlebar_inset() + PANE_PADDING,
                    right: PANE_PADDING,
                    bottom: PANE_PADDING,
                    left: PANE_PADDING,
                }),
        )
        .width(Length::Fill)
        .height(Length::Fill);

        let body = column(vec![pane.into(), self.footer(colors, radius)])
            .width(Length::Fill)
            .height(Length::Fill);

        let content = row(vec![sidebar.into(), body.into()])
            .width(Length::Fill)
            .height(Length::Fill);

        container(content)
            .style(theme::settings_window)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn sidebar(&self, radius: f32) -> Element<'_, Message> {
        let tabs: Vec<Element<'_, Message>> = Tab::ALL
            .iter()
            .map(|tab| self.tab_button(*tab, radius))
            .collect();

        column(tabs)
            .spacing(2)
            .padding(Padding {
                top: crate::platform::titlebar_inset() + 12.0,
                right: 8.0,
                bottom: 12.0,
                left: 8.0,
            })
            .into()
    }

    fn tab_button(&self, tab: Tab, radius: f32) -> Element<'_, Message> {
        let selected = self.tab == tab;

        button(
            container(text(tab.label()).size(LABEL_SIZE))
                .width(Length::Fill)
                .align_x(Horizontal::Left),
        )
        .width(Length::Fill)
        .padding([6, 10])
        .on_press(Message::Tab(tab))
        .style(move |theme, status| theme::settings_tab(theme, status, selected, radius))
        .into()
    }

    fn pane(&self, colors: theme::Colors, radius: f32) -> Element<'_, Message> {
        match self.tab {
            Tab::Behavior => self.behavior_pane(colors, radius),
            Tab::Appearance => self.appearance_pane(colors, radius),
            Tab::Advanced => self.advanced_pane(colors, radius),
            Tab::About => self.about_pane(colors, radius),
        }
    }

    fn behavior_pane(&self, colors: theme::Colors, radius: f32) -> Element<'_, Message> {
        let mut items: Vec<Element<'_, Message>> = vec![
            note("Copy and move", colors),
            card(
                vec![
                    setting_row(
                        "Move modifier",
                        "Hold this key while dropping to move the file instead of copying it.",
                        pick_list(
                            &MODIFIER_LABELS[..],
                            Some(modifier_label(self.move_modifier)),
                            |label| Message::MoveModifier(modifier_of(label)),
                        )
                        .style(move |theme, status| {
                            theme::settings_pick_list(theme, status, radius)
                        })
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                        colors,
                    ),
                    setting_row(
                        "Invert copy and move",
                        "Swap which action happens by default and which one needs the modifier.",
                        checkbox(self.invert_copy_and_move)
                            .on_toggle(Message::InvertCopyAndMove)
                            .into(),
                        colors,
                    ),
                ],
                radius,
            ),
            note("Source filter", colors),
            note(
                "Rules are checked from top to bottom against the app and window title \
                 a drag started from. The first match wins; if nothing matches, the drag \
                 is accepted. Both patterns are regular expressions.",
                colors,
            ),
        ];

        for (index, rule) in self.rules.iter().enumerate() {
            items.push(self.rule_card(index, rule, colors, radius));
        }

        items.push(
            button(text("Add rule").size(LABEL_SIZE))
                .on_press(Message::RuleAdd)
                .style(move |theme, status| theme::settings_button(theme, status, radius))
                .padding([6, 12])
                .into(),
        );

        column(items).spacing(12).into()
    }

    fn rule_card(
        &self,
        index: usize,
        rule: &RuleDraft,
        colors: theme::Colors,
        radius: f32,
    ) -> Element<'_, Message> {
        let header = row(vec![
            text(format!("Rule {}", index + 1)).size(LABEL_SIZE).into(),
            space().width(Length::Fill).into(),
            button(text("Remove").size(NOTE_SIZE))
                .on_press(Message::RuleRemove(index))
                .style(move |theme, status| theme::settings_button(theme, status, radius))
                .padding([4, 10])
                .into(),
        ])
        .align_y(Alignment::Center);

        card(
            vec![
                header.into(),
                setting_row(
                    "App",
                    "Matched against the name of the app the drag started from.",
                    text_input("", &rule.app)
                        .on_input(move |value| Message::RuleApp(index, value))
                        .style(move |theme, status| theme::settings_input(theme, status, radius))
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                    colors,
                ),
                setting_row(
                    "Title",
                    "Matched against the title of the window the drag started from.",
                    text_input("", &rule.title)
                        .on_input(move |value| Message::RuleTitle(index, value))
                        .style(move |theme, status| theme::settings_input(theme, status, radius))
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                    colors,
                ),
                setting_row(
                    "Action",
                    "Whether a drag that matches this rule is accepted or rejected.",
                    pick_list(
                        &ACTION_LABELS[..],
                        Some(action_label(rule.action)),
                        move |label| Message::RuleAction(index, action_of(label)),
                    )
                    .style(move |theme, status| theme::settings_pick_list(theme, status, radius))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    colors,
                ),
            ],
            radius,
        )
    }

    fn appearance_pane(&self, colors: theme::Colors, radius: f32) -> Element<'_, Message> {
        column(vec![
            note("Window", colors),
            card(
                vec![setting_row(
                    "Side",
                    "Which edge of the screen the tray slides in from.",
                    pick_list(&SIDE_LABELS[..], Some(side_label(self.side)), |label| {
                        Message::Side(side_of(label))
                    })
                    .style(move |theme, status| theme::settings_pick_list(theme, status, radius))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    colors,
                )],
                radius,
            ),
            note("Theme", colors),
            card(
                vec![setting_row(
                    "Theme",
                    "Follow the system appearance, or force a light or dark theme.",
                    pick_list(&THEME_LABELS[..], Some(theme_label(self.theme)), |label| {
                        Message::Theme(theme_of(label))
                    })
                    .style(move |theme, status| theme::settings_pick_list(theme, status, radius))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    colors,
                )],
                radius,
            ),
        ])
        .spacing(12)
        .into()
    }

    fn advanced_pane(&self, colors: theme::Colors, radius: f32) -> Element<'_, Message> {
        column(vec![
            note("Cache", colors),
            card(
                vec![setting_row(
                    "Cache directory",
                    "Where files are kept after they are moved into the tray.",
                    text_input("", &self.cache_dir)
                        .on_input(Message::CacheDir)
                        .style(move |theme, status| theme::settings_input(theme, status, radius))
                        .width(Length::Fixed(WIDE_CONTROL_WIDTH))
                        .into(),
                    colors,
                )],
                radius,
            ),
            note(
                "Leave empty to keep moved files in a temporary directory that is deleted \
                 when atray quits.",
                colors,
            ),
        ])
        .spacing(12)
        .into()
    }

    fn about_pane(&self, colors: theme::Colors, radius: f32) -> Element<'_, Message> {
        let mut header: Vec<Element<'_, Message>> = Vec::new();

        if let Some(icon) = &self.icon {
            header.push(
                container(
                    image(icon.clone())
                        .width(Length::Fixed(ICON_RENDER_SIZE))
                        .height(Length::Fixed(ICON_RENDER_SIZE))
                        .content_fit(ContentFit::Contain),
                )
                .center_x(Length::Fill)
                .into(),
            );
        }

        header.push(
            container(text(env!("CARGO_PKG_NAME")).size(TITLE_SIZE))
                .center_x(Length::Fill)
                .into(),
        );
        header.push(
            container(
                text(env!("CARGO_PKG_DESCRIPTION"))
                    .size(NOTE_SIZE)
                    .color(colors.note())
                    .wrapping(Wrapping::Word),
            )
            .center_x(Length::Fill)
            .padding([0.0, 24.0])
            .into(),
        );

        let rows = card(
            vec![
                setting_row(
                    "Version",
                    "",
                    text(env!("CARGO_PKG_VERSION")).size(LABEL_SIZE).into(),
                    colors,
                ),
                setting_row("License", "", text(LICENSE).size(LABEL_SIZE).into(), colors),
            ],
            radius,
        );

        column(vec![
            column(header).spacing(6).into(),
            rows.into(),
            link_buttons(radius).into(),
        ])
        .spacing(20)
        .into()
    }

    fn footer(&self, colors: theme::Colors, radius: f32) -> Element<'_, Message> {
        let status: Element<'_, Message> = match &self.error {
            Some(error) => text(error.as_str())
                .size(NOTE_SIZE)
                .color(colors.danger)
                .into(),
            None => space().into(),
        };

        let buttons = row(vec![
            button(text("Revert").size(LABEL_SIZE))
                .on_press(Message::Revert)
                .style(move |theme, status| theme::settings_button(theme, status, radius))
                .padding([6, 12])
                .into(),
            button(text("Save").size(LABEL_SIZE))
                .on_press(Message::Save)
                .style(move |theme, status| theme::settings_primary_button(theme, status, radius))
                .padding([6, 12])
                .into(),
        ])
        .spacing(8);

        container(
            row(vec![
                status,
                space().width(Length::Fill).into(),
                buttons.into(),
            ])
            .align_y(Alignment::Center),
        )
        .padding([12.0, PANE_PADDING])
        .width(Length::Fill)
        .into()
    }
}

pub fn window_settings() -> window::Settings {
    window::Settings {
        size: (540.0, 600.0).into(),
        position: window::Position::Centered,
        resizable: false,
        transparent: true,
        blur: true,
        platform_specific: crate::platform::platform_window_settings(),
        exit_on_close_request: false,
        ..Default::default()
    }
}

fn card<'a>(rows: Vec<Element<'a, Message>>, radius: f32) -> Element<'a, Message> {
    let mut column = Column::new().spacing(0);

    for (index, item) in rows.into_iter().enumerate() {
        if index > 0 {
            column = column.push(rule::horizontal(1.0));
        }

        column = column.push(item);
    }

    container(column)
        .style(move |theme| theme::settings_card(theme, radius))
        .padding([4, 12])
        .width(Length::Fill)
        .into()
}

fn setting_row<'a>(
    label: &'a str,
    description: &'a str,
    control: Element<'a, Message>,
    colors: theme::Colors,
) -> Element<'a, Message> {
    let mut labels = Column::new()
        .spacing(2)
        .width(Length::Fill)
        .push(text(label).size(LABEL_SIZE));

    if !description.is_empty() {
        labels = labels.push(text(description).size(NOTE_SIZE).color(colors.note()));
    }

    row(vec![labels.into(), control])
        .align_y(Alignment::Center)
        .padding([10, 0])
        .into()
}

fn note<'a>(label: &'a str, colors: theme::Colors) -> Element<'a, Message> {
    text(label).size(NOTE_SIZE).color(colors.note()).into()
}

fn icon_handle() -> Option<Handle> {
    static HANDLE: std::sync::OnceLock<Option<Handle>> = std::sync::OnceLock::new();

    HANDLE
        .get_or_init(|| {
            const SIZE: u32 = 192;

            let icon = ::image::load_from_memory(ICON).ok()?;
            let icon = icon
                .resize(SIZE, SIZE, ::image::imageops::FilterType::Lanczos3)
                .to_rgba8();
            let (width, height) = icon.dimensions();

            Some(Handle::from_rgba(width, height, icon.into_raw()))
        })
        .clone()
}

fn modifier_label(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Alt => "Alt",
        Modifier::Control => "Control",
        Modifier::Shift => "Shift",
        Modifier::Super => "Super",
    }
}

fn modifier_of(label: &str) -> Modifier {
    match label {
        "Alt" => Modifier::Alt,
        "Control" => Modifier::Control,
        "Super" => Modifier::Super,
        _ => Modifier::Shift,
    }
}

fn side_label(side: Side) -> &'static str {
    match side {
        Side::Left => "Left",
        Side::Right => "Right",
        Side::Top => "Top",
        Side::Bottom => "Bottom",
    }
}

fn side_of(label: &str) -> Side {
    match label {
        "Right" => Side::Right,
        "Top" => Side::Top,
        "Bottom" => Side::Bottom,
        _ => Side::Left,
    }
}

fn theme_label(theme: ThemeMode) -> &'static str {
    match theme {
        ThemeMode::System => "System",
        ThemeMode::Light => "Light",
        ThemeMode::Dark => "Dark",
    }
}

fn theme_of(label: &str) -> ThemeMode {
    match label {
        "Light" => ThemeMode::Light,
        "Dark" => ThemeMode::Dark,
        _ => ThemeMode::System,
    }
}

fn action_label(action: FilterAction) -> &'static str {
    match action {
        FilterAction::Allow => "Allow",
        FilterAction::Deny => "Deny",
    }
}

fn action_of(label: &str) -> FilterAction {
    match label {
        "Allow" => FilterAction::Allow,
        _ => FilterAction::Deny,
    }
}

fn link_buttons<'a>(radius: f32) -> Element<'a, Message> {
    row![
        button(text("Source").align_x(iced::alignment::Horizontal::Center))
            .on_press(Message::OpenLink(SOURCE))
            .width(Length::Fill)
            .style(move |theme, status| theme::settings_button(theme, status, radius)),
        button(text("Issues").align_x(iced::alignment::Horizontal::Center))
            .on_press(Message::OpenLink(ISSUES))
            .width(Length::Fill)
            .style(move |theme, status| theme::settings_button(theme, status, radius))
    ]
    .spacing(6)
    .width(Length::Fill)
    .into()
}
