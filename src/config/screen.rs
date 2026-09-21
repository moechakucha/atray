use iced::advanced::image::Handle;
use iced::advanced::text::Wrapping;
use iced::alignment::Horizontal;
use iced::widget::{
    Column, button, column, container, image, pick_list, row, rule, scrollable, space, text,
    text_input, toggler,
};
use iced::{Alignment, ContentFit, Element, Length, Padding, window};

use fluent::FluentValue;

use crate::i18n;
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
const SOURCE: &str = "https://git.sr.ht/~flamarine/atray";
const ISSUES: &str = "https://todo.sr.ht/~flamarine/atray";

#[cfg(not(target_os = "windows"))]
const MODIFIERS: [Modifier; 4] = [
    Modifier::Alt,
    Modifier::Control,
    Modifier::Shift,
    Modifier::Super,
];
#[cfg(target_os = "windows")]
const MODIFIERS: [Modifier; 3] = [Modifier::Alt, Modifier::Control, Modifier::Shift];
const SIDES: [Side; 4] = [Side::Left, Side::Right, Side::Top, Side::Bottom];
const THEMES: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];
const ACTIONS: [FilterAction; 2] = [FilterAction::Allow, FilterAction::Deny];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Behavior,
    Appearance,
    Advanced,
    About,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Behavior, Tab::Appearance, Tab::Advanced, Tab::About];

    fn label(self) -> String {
        i18n::t(match self {
            Tab::Behavior => "tab-behavior",
            Tab::Appearance => "tab-appearance",
            Tab::Advanced => "tab-advanced",
            Tab::About => "tab-about",
        })
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
    Language(Option<String>),
    CacheDir(String),
    LaunchAtLogin(bool),
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
        let index = FluentValue::from(index + 1);

        Ok(FilterRule {
            app: pattern(&self.app).map_err(|error| {
                i18n::t_args(
                    "error-rule-app",
                    &[("index", index.clone()), ("error", error.into())],
                )
            })?,
            title: pattern(&self.title).map_err(|error| {
                i18n::t_args(
                    "error-rule-window-title",
                    &[("index", index.clone()), ("error", error.into())],
                )
            })?,
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
    language: Option<String>,
    cache_dir: String,
    launch_at_login: bool,
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
            language: config.appearance.language.clone(),
            cache_dir: config.advanced.cache_dir.clone(),
            launch_at_login: config.advanced.launch_at_login,
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
        config.appearance.language = self.language.clone();
        config.advanced.cache_dir = self.cache_dir.trim().to_owned();
        config.advanced.launch_at_login = self.launch_at_login;

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
            Message::Language(language) => self.language = language,
            Message::CacheDir(value) => self.cache_dir = value,
            Message::LaunchAtLogin(enabled) => self.launch_at_login = enabled,
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

    pub fn view(&self, colors: theme::Colors, metrics: theme::Metrics) -> Element<'_, Message> {
        let sidebar = container(self.sidebar(metrics))
            .style(theme::settings_sidebar)
            .width(SIDEBAR_WIDTH)
            .height(Length::Fill);

        let pane = scrollable(
            container(self.pane(colors, metrics))
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

        let body = column(vec![pane.into(), self.footer(colors, metrics)])
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

    fn sidebar(&self, metrics: theme::Metrics) -> Element<'_, Message> {
        let tabs: Vec<Element<'_, Message>> = Tab::ALL
            .iter()
            .map(|tab| self.tab_button(*tab, metrics))
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

    fn tab_button(&self, tab: Tab, metrics: theme::Metrics) -> Element<'_, Message> {
        let selected = self.tab == tab;

        button(
            container(text(tab.label()).size(LABEL_SIZE))
                .width(Length::Fill)
                .align_x(Horizontal::Left),
        )
        .width(Length::Fill)
        .padding([6, 10])
        .on_press(Message::Tab(tab))
        .style(move |theme, status| theme::settings_tab(theme, status, selected, metrics))
        .into()
    }

    fn pane(&self, colors: theme::Colors, metrics: theme::Metrics) -> Element<'_, Message> {
        match self.tab {
            Tab::Behavior => self.behavior_pane(colors, metrics),
            Tab::Appearance => self.appearance_pane(colors, metrics),
            Tab::Advanced => self.advanced_pane(colors, metrics),
            Tab::About => self.about_pane(colors, metrics),
        }
    }

    fn behavior_pane(
        &self,
        colors: theme::Colors,
        metrics: theme::Metrics,
    ) -> Element<'_, Message> {
        let mut items: Vec<Element<'_, Message>> = vec![
            note(i18n::t("behavior-copy-and-move"), colors),
            card(
                vec![
                    setting_row(
                        i18n::t("behavior-move-modifier"),
                        i18n::t("behavior-move-modifier-description"),
                        pick_list(
                            options(&MODIFIERS, modifier_label),
                            Some(modifier_label(self.move_modifier)),
                            |label| Message::MoveModifier(modifier_of(&label)),
                        )
                        .style(move |theme, status| {
                            theme::settings_pick_list(theme, status, metrics)
                        })
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                        colors,
                    ),
                    setting_row(
                        i18n::t("behavior-invert"),
                        i18n::t("behavior-invert-description"),
                        toggler(self.invert_copy_and_move)
                            .on_toggle(Message::InvertCopyAndMove)
                            .into(),
                        colors,
                    ),
                ],
                metrics,
            ),
            note(i18n::t("behavior-source-filter"), colors),
            note(i18n::t("behavior-source-filter-description"), colors),
        ];

        for (index, rule) in self.rules.iter().enumerate() {
            items.push(self.rule_card(index, rule, colors, metrics));
        }

        items.push(
            button(text(i18n::t("behavior-add-rule")).size(LABEL_SIZE))
                .on_press(Message::RuleAdd)
                .style(move |theme, status| theme::settings_button(theme, status, metrics))
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
        metrics: theme::Metrics,
    ) -> Element<'_, Message> {
        let header = row(vec![
            text(i18n::t_args(
                "rule-heading",
                &[("index", FluentValue::from(index + 1))],
            ))
            .size(LABEL_SIZE)
            .into(),
            space().width(Length::Fill).into(),
            button(text(i18n::t("rule-remove")).size(NOTE_SIZE))
                .on_press(Message::RuleRemove(index))
                .style(move |theme, status| theme::settings_button(theme, status, metrics))
                .padding([4, 10])
                .into(),
        ])
        .align_y(Alignment::Center);

        card(
            vec![
                header.into(),
                setting_row(
                    i18n::t("rule-app"),
                    i18n::t("rule-app-description"),
                    text_input("", &rule.app)
                        .on_input(move |value| Message::RuleApp(index, value))
                        .style(move |theme, status| theme::settings_input(theme, status, metrics))
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                    colors,
                ),
                setting_row(
                    i18n::t("rule-window-title"),
                    i18n::t("rule-window-title-description"),
                    text_input("", &rule.title)
                        .on_input(move |value| Message::RuleTitle(index, value))
                        .style(move |theme, status| theme::settings_input(theme, status, metrics))
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .into(),
                    colors,
                ),
                setting_row(
                    i18n::t("rule-action"),
                    i18n::t("rule-action-description"),
                    pick_list(
                        options(&ACTIONS, action_label),
                        Some(action_label(rule.action)),
                        move |label| Message::RuleAction(index, action_of(&label)),
                    )
                    .style(move |theme, status| theme::settings_pick_list(theme, status, metrics))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    colors,
                ),
            ],
            metrics,
        )
    }

    fn appearance_pane(
        &self,
        colors: theme::Colors,
        metrics: theme::Metrics,
    ) -> Element<'_, Message> {
        let language = language_options();
        let language_labels: Vec<String> =
            language.iter().map(|(label, _)| label.clone()).collect();

        column(vec![
            note(i18n::t("appearance-window"), colors),
            card(
                vec![setting_row(
                    i18n::t("appearance-side"),
                    i18n::t("appearance-side-description"),
                    pick_list(
                        options(&SIDES, side_label),
                        Some(side_label(self.side)),
                        |label| Message::Side(side_of(&label)),
                    )
                    .style(move |theme, status| theme::settings_pick_list(theme, status, metrics))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    colors,
                )],
                metrics,
            ),
            note(i18n::t("appearance-theme"), colors),
            card(
                vec![setting_row(
                    i18n::t("appearance-theme"),
                    i18n::t("appearance-theme-description"),
                    pick_list(
                        options(&THEMES, theme_label),
                        Some(theme_label(self.theme)),
                        |label| Message::Theme(theme_of(&label)),
                    )
                    .style(move |theme, status| theme::settings_pick_list(theme, status, metrics))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    colors,
                )],
                metrics,
            ),
            note(i18n::t("appearance-language"), colors),
            card(
                vec![setting_row(
                    i18n::t("appearance-language"),
                    i18n::t("appearance-language-description"),
                    pick_list(
                        language_labels,
                        Some(language_label(self.language.as_deref())),
                        move |label| {
                            Message::Language(
                                language
                                    .iter()
                                    .find(|(name, _)| *name == label)
                                    .and_then(|(_, language)| language.clone()),
                            )
                        },
                    )
                    .style(move |theme, status| theme::settings_pick_list(theme, status, metrics))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .into(),
                    colors,
                )],
                metrics,
            ),
        ])
        .spacing(12)
        .into()
    }

    fn advanced_pane(
        &self,
        colors: theme::Colors,
        metrics: theme::Metrics,
    ) -> Element<'_, Message> {
        column(vec![
            note(i18n::t("advanced-cache"), colors),
            card(
                vec![setting_row(
                    i18n::t("advanced-cache-directory"),
                    i18n::t("advanced-cache-directory-description"),
                    text_input("", &self.cache_dir)
                        .on_input(Message::CacheDir)
                        .style(move |theme, status| theme::settings_input(theme, status, metrics))
                        .width(Length::Fixed(WIDE_CONTROL_WIDTH))
                        .into(),
                    colors,
                )],
                metrics,
            ),
            note(i18n::t("advanced-cache-directory-note"), colors),
            note(i18n::t("advanced-startup"), colors),
            card(
                vec![setting_row(
                    i18n::t("advanced-launch-at-login"),
                    i18n::t("advanced-launch-at-login-description"),
                    toggler(self.launch_at_login)
                        .on_toggle(Message::LaunchAtLogin)
                        .into(),
                    colors,
                )],
                metrics,
            ),
        ])
        .spacing(12)
        .into()
    }

    fn about_pane(&self, colors: theme::Colors, metrics: theme::Metrics) -> Element<'_, Message> {
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
                text(i18n::t_args(
                    "about-version",
                    &[("version", env!("CARGO_PKG_VERSION").into())],
                ))
                .size(NOTE_SIZE)
                .color(colors.note())
                .wrapping(Wrapping::Word),
            )
            .center_x(Length::Fill)
            .padding([0.0, 24.0])
            .into(),
        );
        header.push(
            container(
                text(i18n::t("about-description"))
                    .size(NOTE_SIZE)
                    .color(colors.note())
                    .wrapping(Wrapping::Word),
            )
            .center_x(Length::Fill)
            .padding([0.0, 24.0])
            .into(),
        );

        column(vec![
            column(header).spacing(6).into(),
            link_buttons(metrics).into(),
            note(i18n::t("about-copyright"), colors).into(),
        ])
        .spacing(20)
        .into()
    }

    fn footer(&self, colors: theme::Colors, metrics: theme::Metrics) -> Element<'_, Message> {
        let status: Element<'_, Message> = match &self.error {
            Some(error) => text(error.as_str())
                .size(NOTE_SIZE)
                .color(colors.danger)
                .into(),
            None => space().into(),
        };

        let buttons = row(vec![
            button(text(i18n::t("footer-revert")).size(LABEL_SIZE))
                .on_press(Message::Revert)
                .style(move |theme, status| theme::settings_button(theme, status, metrics))
                .padding([6, 12])
                .into(),
            button(text(i18n::t("footer-save")).size(LABEL_SIZE))
                .on_press(Message::Save)
                .style(move |theme, status| theme::settings_primary_button(theme, status, metrics))
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
        blur: false,
        icon: window_icon(),
        platform_specific: crate::platform::platform_window_settings(),
        exit_on_close_request: false,
        ..Default::default()
    }
}

fn window_icon() -> Option<window::Icon> {
    static WINDOW_ICON: std::sync::OnceLock<Option<window::Icon>> = std::sync::OnceLock::new();

    WINDOW_ICON
        .get_or_init(|| {
            let size = crate::platform::window_icon_size();

            let icon = ::image::load_from_memory(ICON).ok()?;
            let icon = icon
                .resize(size, size, ::image::imageops::FilterType::Lanczos3)
                .to_rgba8();
            let (width, height) = icon.dimensions();

            window::icon::from_rgba(icon.into_raw(), width, height).ok()
        })
        .clone()
}

fn card<'a>(rows: Vec<Element<'a, Message>>, metrics: theme::Metrics) -> Element<'a, Message> {
    let mut column = Column::new().spacing(0);

    for (index, item) in rows.into_iter().enumerate() {
        if index > 0 {
            column = column.push(rule::horizontal(metrics.line_width));
        }

        column = column.push(item);
    }

    container(column)
        .style(move |theme| theme::settings_card(theme, metrics))
        .padding([4, 12])
        .width(Length::Fill)
        .into()
}

fn setting_row<'a>(
    label: String,
    description: String,
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

fn note<'a>(label: String, colors: theme::Colors) -> Element<'a, Message> {
    text(label).size(NOTE_SIZE).color(colors.note()).into()
}

fn options<T: Copy>(values: &[T], label: fn(T) -> String) -> Vec<String> {
    values.iter().map(|value| label(*value)).collect()
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

fn modifier_label(modifier: Modifier) -> String {
    i18n::t(match modifier {
        Modifier::Alt => "modifier-alt",
        Modifier::Control => "modifier-control",
        Modifier::Shift => "modifier-shift",
        Modifier::Super => "modifier-super",
    })
}

fn modifier_of(label: &str) -> Modifier {
    MODIFIERS
        .into_iter()
        .find(|modifier| modifier_label(*modifier) == label)
        .unwrap_or(Modifier::Shift)
}

fn side_label(side: Side) -> String {
    i18n::t(match side {
        Side::Left => "side-left",
        Side::Right => "side-right",
        Side::Top => "side-top",
        Side::Bottom => "side-bottom",
    })
}

fn side_of(label: &str) -> Side {
    SIDES
        .into_iter()
        .find(|side| side_label(*side) == label)
        .unwrap_or(Side::Left)
}

fn theme_label(theme: ThemeMode) -> String {
    i18n::t(match theme {
        ThemeMode::System => "theme-system",
        ThemeMode::Light => "theme-light",
        ThemeMode::Dark => "theme-dark",
    })
}

fn theme_of(label: &str) -> ThemeMode {
    THEMES
        .into_iter()
        .find(|theme| theme_label(*theme) == label)
        .unwrap_or(ThemeMode::System)
}

fn action_label(action: FilterAction) -> String {
    i18n::t(match action {
        FilterAction::Allow => "action-allow",
        FilterAction::Deny => "action-deny",
    })
}

fn action_of(label: &str) -> FilterAction {
    ACTIONS
        .into_iter()
        .find(|action| action_label(*action) == label)
        .unwrap_or(FilterAction::Deny)
}

fn language_options() -> Vec<(String, Option<String>)> {
    let mut options = vec![(i18n::t("language-system"), None)];

    options.extend(
        i18n::locales()
            .into_iter()
            .map(|locale| (i18n::name(&locale), Some(locale.to_string()))),
    );

    options
}

fn language_label(language: Option<&str>) -> String {
    let Some(language) = language else {
        return i18n::t("language-system");
    };

    language_options()
        .into_iter()
        .find(|(_, value)| value.as_deref() == Some(language))
        .map(|(label, _)| label)
        .unwrap_or_else(|| language.to_string())
}

fn link_buttons<'a>(metrics: theme::Metrics) -> Element<'a, Message> {
    row![
        button(text(i18n::t("about-source")).align_x(iced::alignment::Horizontal::Center))
            .on_press(Message::OpenLink(SOURCE))
            .width(Length::Fill)
            .style(move |theme, status| theme::settings_button(theme, status, metrics)),
        button(text(i18n::t("about-issues")).align_x(iced::alignment::Horizontal::Center))
            .on_press(Message::OpenLink(ISSUES))
            .width(Length::Fill)
            .style(move |theme, status| theme::settings_button(theme, status, metrics))
    ]
    .spacing(6)
    .width(Length::Fill)
    .into()
}
