language-name = 简体中文

menu-version = 版本 { $version }
menu-settings = 设置...
menu-open-config = 打开配置文件
menu-reload-config = 重新加载配置文件
menu-quit = 退出

settings-window-title = atray 设置

tab-behavior = 行为
tab-appearance = 外观
tab-advanced = 高级
tab-about = 关于

behavior-copy-and-move = 复制与移动
behavior-move-modifier = 移动修饰键
behavior-move-modifier-description = 按住此键拖放时会移动文件，而不是复制。
behavior-invert = 反转复制与移动
behavior-invert-description = 交换默认执行的操作和需要修饰键的操作。
behavior-source-filter = 来源过滤
behavior-source-filter-description = 规则会自上而下与拖拽来源的应用名称和窗口标题进行匹配。首个匹配的规则生效；若都不匹配，则接受该拖拽。两个模式均为正则表达式。
behavior-add-rule = 添加规则

rule-heading = 规则 { $index }
rule-remove = 移除
rule-app = 应用
rule-app-description = 与拖拽来源的应用名称进行匹配。
rule-window-title = 标题
rule-window-title-description = 与拖拽来源的窗口标题进行匹配。
rule-action = 操作
rule-action-description = 指定匹配该规则的拖拽是被接受还是被拒绝。

action-allow = 允许
action-deny = 拒绝

modifier-alt = Alt
modifier-control = Control
modifier-shift = Shift
modifier-super = Super

appearance-window = 窗口
appearance-side = 边缘
appearance-side-description = 托盘从屏幕的哪一侧滑入。
side-left = 左侧
side-right = 右侧
side-top = 顶部
side-bottom = 底部

appearance-theme = 主题
appearance-theme-description = 跟随系统外观，或强制使用浅色或深色主题。
theme-system = 跟随系统
theme-light = 浅色
theme-dark = 深色

appearance-language = 语言
appearance-language-description = 跟随系统语言，或手动指定一种语言。
language-system = 跟随系统

advanced-cache = 缓存
advanced-cache-directory = 缓存目录
advanced-cache-directory-description = 文件被移入托盘后的存放位置。
advanced-cache-directory-note = 留空则改用临时目录，该目录会在退出 atray 时删除。

advanced-startup = 启动
advanced-launch-at-login = 登录时启动
advanced-launch-at-login-description = 登录后自动启动托盘。

about-version = 版本 { $version }
about-description = 简洁的文件中转托盘；Yoink 的跨平台替代品
about-source = 源码
about-issues = 问题反馈
about-copyright = © 2026 moechakucha。基于 GNU 通用公共许可证 v3.0 授权。

error-rule-app = 规则 { $index }：应用：{ $error }
error-rule-window-title = 规则 { $index }：标题：{ $error }
error-launch-at-login = 更新登录项失败：{ $error }

file-unknown-name = 未知文件
