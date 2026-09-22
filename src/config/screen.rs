use iced::advanced::image::Handle;
use iced::advanced::text::Wrapping;
use iced::alignment::Horizontal;
use iced::widget::{
    Column, button, column, container, image, pick_list, row, rule, scrollable, space, text,
    text_input, toggler,
};
use iced::{Alignment, ContentFit, Element, Length, Padding, window};

use fluent::FluentValue;

use crate::config::FilterAction::Allow;
use crate::i18n;
use crate::input::Modifier;
use crate::theme;

use super::{Config, FilterAction, FilterRule, Pattern, Side, ThemeMode};

const SIDEBAR_WIDTH: f32 = 135.0;
const CONTROL_WIDTH: f32 = 120.0;
const WIDE_CONTROL_WIDTH: f32 = 180.0;
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
    rules: Vec<RuleDraft>,
    cache_dir: String,
    error: Option<String>,
    icon: Option<Handle>,
}

impl Screen {
    pub fn from_config(config: &Config) -> Self {
        Self {
            tab: Tab::Behavior,
            rules: config
                .behavior
                .filter
                .iter()
                .map(RuleDraft::from_rule)
                .collect(),
            cache_dir: config.advanced.cache_dir.clone(),
            error: None,
            icon: icon_handle(),
        }
    }

    fn sync_filter(&mut self, config: &mut Config) -> bool {
        let mut filter = Vec::with_capacity(self.rules.len());

        for (index, rule) in self.rules.iter().enumerate() {
            match rule.to_rule(index) {
                Ok(rule) => filter.push(rule),
                Err(error) => {
                    self.error = Some(error);
                    return false;
                }
            }
        }

        self.error = None;

        if config.behavior.filter == filter {
            return false;
        }

        config.behavior.filter = filter;

        true
    }

    pub fn update(&mut self, message: Message, config: &mut Config) -> bool {
        match message {
            Message::Tab(tab) => {
                self.tab = tab;
                false
            }
            Message::MoveModifier(modifier) => {
                config.behavior.move_modifier = modifier;
                true
            }
            Message::InvertCopyAndMove(value) => {
                config.behavior.invert_copy_and_move = value;
                true
            }
            Message::RuleApp(index, value) => {
                if let Some(rule) = self.rules.get_mut(index) {
                    rule.app = value;
                }

                self.sync_filter(config)
            }
            Message::RuleTitle(index, value) => {
                if let Some(rule) = self.rules.get_mut(index) {
                    rule.title = value;
                }

                self.sync_filter(config)
            }
            Message::RuleAction(index, action) => {
                if let Some(rule) = self.rules.get_mut(index) {
                    rule.action = action;
                }

                self.sync_filter(config)
            }
            Message::RuleRemove(index) => {
                if index < self.rules.len() {
                    self.rules.remove(index);
                }

                self.sync_filter(config)
            }
            Message::RuleAdd => {
                self.rules.push(RuleDraft {
                    app: String::new(),
                    title: String::new(),
                    action: Allow,
                });

                self.sync_filter(config)
            }
            Message::Side(side) => {
                config.appearance.side = side;
                true
            }
            Message::Theme(theme) => {
                config.appearance.theme = theme;
                true
            }
            Message::Language(language) => {
                config.appearance.language = language;
                true
            }
            Message::CacheDir(value) => {
                self.cache_dir = value;

                let cache_dir = self.cache_dir.trim().to_owned();

                if config.advanced.cache_dir == cache_dir {
                    return false;
                }

                config.advanced.cache_dir = cache_dir;

                true
            }
            Message::LaunchAtLogin(enabled) => {
                config.advanced.launch_at_login = enabled;
                true
            }
            Message::OpenLink(url) => {
                let _ = open::that(url);
                false
            }
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn view(
        &self,
        config: &Config,
        colors: theme::Colors,
        metrics: theme::Metrics,
    ) -> Element<'_, Message> {
        let sidebar = container(self.sidebar(metrics))
            .style(theme::settings_sidebar)
            .width(SIDEBAR_WIDTH)
            .height(Length::Fill);

        let pane = scrollable(
            container(self.pane(config, colors, metrics))
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

        let body = column(vec![pane.into(), self.footer(colors)])
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

    fn pane(
        &self,
        config: &Config,
        colors: theme::Colors,
        metrics: theme::Metrics,
    ) -> Element<'_, Message> {
        match self.tab {
            Tab::Behavior => self.behavior_pane(config, colors, metrics),
            Tab::Appearance => self.appearance_pane(config, colors, metrics),
            Tab::Advanced => self.advanced_pane(config, colors, metrics),
            Tab::About => self.about_pane(colors, metrics),
        }
    }

    fn behavior_pane(
        &self,
        config: &Config,
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
                            Some(modifier_label(config.behavior.move_modifier)),
                            |label| Message::MoveModifier(modifier_of(&label)),
                        )
                        .style(move |theme, status| {
                            theme::settings_pick_list(theme, status, metrics)
                        })
                        .width(Length::Fixed(CONTROL_WIDTH))
                        .text_size(LABEL_SIZE)
                        .into(),
                        colors,
                    ),
                    setting_row(
                        i18n::t("behavior-invert"),
                        i18n::t("behavior-invert-description"),
                        toggler(config.behavior.invert_copy_and_move)
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
                        .size(LABEL_SIZE)
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
                        .size(LABEL_SIZE)
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
                    .text_size(LABEL_SIZE)
                    .into(),
                    colors,
                ),
            ],
            metrics,
        )
    }

    fn appearance_pane(
        &self,
        config: &Config,
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
                        Some(side_label(config.appearance.side)),
                        |label| Message::Side(side_of(&label)),
                    )
                    .style(move |theme, status| theme::settings_pick_list(theme, status, metrics))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .text_size(LABEL_SIZE)
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
                        Some(theme_label(config.appearance.theme)),
                        |label| Message::Theme(theme_of(&label)),
                    )
                    .style(move |theme, status| theme::settings_pick_list(theme, status, metrics))
                    .width(Length::Fixed(CONTROL_WIDTH))
                    .text_size(LABEL_SIZE)
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
                        Some(language_label(config.appearance.language.as_deref())),
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
                    .text_size(LABEL_SIZE)
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
        config: &Config,
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
                        .size(LABEL_SIZE)
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
                    toggler(config.advanced.launch_at_login)
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

    fn footer(&self, colors: theme::Colors) -> Element<'_, Message> {
        match &self.error {
            Some(error) => container(
                text(error.as_str())
                    .size(NOTE_SIZE)
                    .color(colors.danger)
                    .wrapping(Wrapping::Word),
            )
            .padding([12.0, PANE_PADDING])
            .width(Length::Fill)
            .into(),
            None => space().into(),
        }
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
