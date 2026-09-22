language-name = English

menu-version = Version { $version }
menu-settings = Settings...
menu-open-config = Open Config File
menu-reload-config = Reload Config File
menu-quit = Quit

settings-window-title = atray Settings

tab-behavior = Behavior
tab-appearance = Appearance
tab-advanced = Advanced
tab-about = About

behavior-copy-and-move = Copy and move
behavior-move-modifier = Move modifier
behavior-move-modifier-description = Hold this key while dropping to move the file instead of copying it.
behavior-invert = Invert copy and move
behavior-invert-description = Swap which action happens by default and which one needs the modifier.
behavior-source-filter = Source filter
behavior-source-filter-description = Rules are checked from top to bottom against the app and window title a drag started from. The first match wins; if nothing matches, the drag is accepted. Both patterns are regular expressions.
behavior-add-rule = Add rule

rule-heading = Rule { $index }
rule-remove = Remove
rule-app = App
rule-app-description = Matched against the name of the app the drag started from.
rule-window-title = Title
rule-window-title-description = Matched against the title of the window the drag started from.
rule-action = Action
rule-action-description = Whether a drag that matches this rule is accepted or rejected.

action-allow = Allow
action-deny = Deny

modifier-alt = Alt
modifier-control = Control
modifier-shift = Shift
modifier-super = Super

appearance-window = Window
appearance-side = Side
appearance-side-description = Which edge of the screen the tray slides in from.
side-left = Left
side-right = Right
side-top = Top
side-bottom = Bottom

appearance-theme = Theme
appearance-theme-description = Follow the system appearance, or force a light or dark theme.
theme-system = System
theme-light = Light
theme-dark = Dark

appearance-language = Language
appearance-language-description = Follow the system language, or pick one explicitly.
language-system = System

advanced-cache = Cache
advanced-cache-directory = Cache directory
advanced-cache-directory-description = Where files are kept after they are moved into the tray.
advanced-cache-directory-note = Leave empty to keep moved files in a temporary directory that is deleted when atray quits.

advanced-startup = Startup
advanced-launch-at-login = Launch at login
advanced-launch-at-login-description = Start the tray automatically when you log in.

about-version = Version { $version }
about-description = Simple file relay tray; cross-platform alternative to Yoink
about-source = Source
about-issues = Issues
about-copyright = © 2026 moechakucha. Licensed under the GNU General Public License v3.0.

error-rule-app = Rule { $index }: app: { $error }
error-rule-window-title = Rule { $index }: title: { $error }
error-launch-at-login = failed to update launch at login: { $error }

file-unknown-name = unknown file
