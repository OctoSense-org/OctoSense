//! Local search over reviewed built-in destinations. Search text never selects
//! an Android intent, provider key, permission or privileged operation.

pub const PAGE_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingsRoute {
    Appearance, Display, DisplayOptions, Sound, Sounds, SoundFeedback, Wifi, Bluetooth, Apps, DefaultApps, Accounts,
    Dnd, Notifications, NotificationHistory, Privacy, Location, Battery, BatteryPolicy, Storage,
    DateTime, About, System, Updates, AdvancedNetwork, AccessibilityVision, AccessibilityHearing, AccessibilityTextInteraction, CaptionCustom, CaptionLanguage, SystemLanguages, Keyboards,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchEntry {
    pub title: &'static str,
    pub location: &'static str,
    pub route: SettingsRoute,
    keywords: &'static str,
}

macro_rules! entries {
    ($($route:ident, $title:literal, $location:literal, $keywords:literal;)+) => {
        pub const ENTRIES: &[SearchEntry] = &[$(SearchEntry {
            title: $title, location: $location, route: SettingsRoute::$route, keywords: $keywords,
        },)+];
    };
}

entries! {
    Keyboards, "Keyboards", "System / Keyboards", "keyboard input method default enable disable subtype languages IME 键盘 输入法 默认键盘 启用 停用";
    Dnd, "Do Not Disturb schedules and rules", "Notifications / Do Not Disturb", "dnd schedule rules interruption exceptions zen 勿扰 免打扰 规则 日程 例外";
    Dnd, "Allowed interruptions", "Notifications / Do Not Disturb", "calls messages conversations alarms repeat callers 来电 信息 重复来电 允许打扰";
    Appearance, "Themes and colors", "Appearance", "theme palette accent style 主题 外观 配色 颜色";
    Appearance, "Light and dark appearance", "Appearance", "light dark automatic system mode 明亮 深色 浅色 暗色 自动 夜间模式";
    Appearance, "Wallpaper", "Appearance", "wallpaper gradient solid background 壁纸 背景 渐变";
    Display, "Brightness", "Display", "display screen brightness dim 亮度 屏幕 显示 明暗";
    Display, "Automatic brightness", "Display", "adaptive auto brightness sensor 自动亮度 亮度 传感器";
    Display, "Screen timeout", "Display", "sleep timeout turn off screen 锁屏 熄屏 休眠 屏幕超时";
    Display, "Font size", "Display", "font text size scale accessibility 字体 字号 文字大小 字体大小 显示大小";
    DisplayOptions, "Display size", "Display / Display size and Night Light", "display size density dpi zoom 显示大小 屏幕缩放 密度";
    DisplayOptions, "Night Light", "Display / Display size and Night Light", "night light warmth blue filter sunset schedule 护眼 夜览 夜间 护眼模式 色温 蓝光 日落";
    Display, "Screen rotation", "Display", "rotation rotate portrait landscape lock 自动旋转 旋转 横屏 竖屏";
    Sounds, "Ringtones and notification sounds", "Sound / Sounds", "ringtone notification sound alarm silent preview 铃声 来电铃声 通知声音 默认闹钟 静音 试听";
    SoundFeedback, "Sound feedback and haptics", "Sound", "charging lock dial pad tones vibration haptics intensity keyboard 充电声音 锁屏声音 拨号音 振动 震动 触觉 强度 键盘振动";
    Sound, "Media volume", "Sound", "volume media music audio 音量 声音 媒体 音乐";
    Sound, "Ring volume", "Sound", "volume ring phone calls ringtone 铃声音量 铃声 来电 声音";
    Sound, "Alarm volume", "Sound", "volume alarm clock 闹钟 音量";
    Sound, "Notification volume", "Sound", "volume notification alert 通知音量 通知 提示音";
    Sound, "Touch sounds", "Sound", "touch sounds click tap 触摸声音 触摸音效 点击音效";
    Sound, "Touch vibration", "Sound", "vibrate vibration haptic feedback touch 触摸振动 震动 振动 触觉反馈";
    Wifi, "Wi-Fi networks", "Connections / Wi-Fi", "wifi wi fi wireless internet network wlan 无线 网络 无线网络 无线局域网";
    Wifi, "Saved Wi-Fi networks", "Connections / Wi-Fi", "saved wifi wi fi connect forget 已保存网络 保存 无线网络 忘记网络";
    Wifi, "Wi-Fi connection details", "Connections / Wi-Fi", "wifi wi fi ip address signal frequency link speed ip地址 信号 频率 网速 连接详情";
    Bluetooth, "Bluetooth devices", "Connections / Bluetooth", "bluetooth nearby pair paired headset headphones speaker 蓝牙 附近设备 配对 耳机 音箱";
    Bluetooth, "Bluetooth device name", "Connections / Bluetooth", "bluetooth name rename 蓝牙名称 设备名称 重命名";
    Bluetooth, "Bluetooth sharing", "Connections / Bluetooth", "bluetooth contacts messages phonebook sharing 蓝牙共享 联系人 电话簿 短信 信息";
    AdvancedNetwork, "Airplane mode", "Connections / Advanced connections", "airplane flight radio 飞行模式 飞行 航空 无线电";
    AdvancedNetwork, "Data Saver", "Connections / Advanced connections", "data saver metered background internet 流量节省 节省流量 数据流量 后台流量";
    AdvancedNetwork, "Private DNS", "Connections / Advanced connections", "private dns encrypted hostname domain name server 私人DNS 私人dns 私有dns 加密DNS 加密 域名解析";
    DefaultApps, "Default apps", "Apps / Default apps", "default browser home launcher assistant phone sms wallet 默认应用 默认浏览器 桌面 启动器 数字助理 电话 短信 钱包";
    Apps, "Installed apps", "Apps", "apps applications installed search uninstall 应用 程序 已安装应用 卸载 应用管理";
    Apps, "App permissions", "Apps / App details", "app permission granted denied camera microphone 应用权限 权限 相机 麦克风";
    Apps, "App battery usage", "Apps / App details / Battery usage", "battery optimization restricted optimized unrestricted background 应用电池 电池优化 后台限制 不受限制";
    Apps, "App network access", "Apps / App details / Network access", "data saver background unrestricted wifi mobile vpn 应用网络 联网控制 后台数据 不限流量";
    Apps, "App language", "Apps / App details / Language", "app language locale regional numbering system default 应用语言 语言 地区 数字格式 系统默认";
    Apps, "App storage", "Apps / App details", "app storage cache data usage 应用存储 存储 缓存 数据 用量";
    Accounts, "Accounts", "Accounts", "account login provider google gmail 帐号 账号 帐户 账户 登录 谷歌 邮箱";
    Accounts, "Add account", "Accounts", "account add sign in provider 添加帐号 添加账号 添加账户 登录";
    Accounts, "Automatic sync", "Accounts", "account automatic sync synchronization 同步 自动同步 帐号同步 账户同步";
    Accounts, "Account sync status", "Accounts / Account details", "sync pending active cancel data 同步状态 正在同步 等待同步 取消同步";
    Dnd, "Do Not Disturb", "Notifications", "dnd do not disturb priority alarms silence interruptions 勿扰 免打扰 优先通知 仅闹钟 完全静音";
    Notifications, "Lock screen notifications", "Notifications", "notification lockscreen show hidden 锁屏通知 通知 显示 隐藏";
    Notifications, "Sensitive notification content", "Notifications", "notification sensitive private content preview 隐私通知 敏感通知 通知内容 通知预览";
    NotificationHistory, "Notification history", "Notifications", "notification history previous 通知历史 历史通知";
    Notifications, "Notification bubbles", "Notifications", "notification bubbles chat floating 通知气泡 气泡 聊天气泡";
    Privacy, "Camera access", "Privacy", "camera access sensor privacy 相机 摄像头 相机访问 隐私";
    Privacy, "Microphone access", "Privacy", "microphone mic audio access sensor privacy 麦克风 录音 麦克风访问 隐私";
    Location, "Location", "Location", "location gps positioning 定位 位置 位置信息";
    Location, "Wi-Fi scanning", "Location", "wifi wi fi scanning location 无线扫描 网络扫描 无线网络扫描";
    Location, "Bluetooth scanning", "Location", "bluetooth scanning location 蓝牙扫描";
    Battery, "Battery status", "Battery", "battery level charging temperature power 电池 电量 充电 温度";
    BatteryPolicy, "Battery Saver", "Battery / Battery saver and policy", "battery saver power save low power 节电 省电 节电模式 省电模式 低电量";
    BatteryPolicy, "Battery Saver schedule", "Battery / Battery saver and policy", "battery saver threshold automatic percent percentage 自动省电 省电阈值 电量百分比";
    BatteryPolicy, "Adaptive Battery", "Battery / Battery saver and policy", "adaptive battery background management 智能电池 自适应电池 后台管理";
    Storage, "Storage capacity", "Storage", "storage capacity available free used disk space 存储 存储空间 容量 剩余空间 可用空间";
    SystemLanguages, "System languages", "System", "language order add remove reorder preferred display regional languages English French Chinese Arabic 系统语言 语言顺序 添加语言";
    CaptionLanguage, "Caption language", "System / Accessibility: hearing", "captions subtitles language locale 字幕 语言";
    CaptionCustom, "Custom caption appearance", "System / Accessibility: hearing", "captions subtitles colors opacity typeface font edge 字幕 颜色 透明度 字体";
    AccessibilityTextInteraction, "Text and interaction", "System / Accessibility: text and interaction", "high contrast bold text animation long press hold time timeout autoclick automatic click large mouse pointer 高对比度 粗体 文字 动画 长按 操作时间 自动点击 鼠标 指针";
    AccessibilityHearing, "Mono audio and audio balance", "System / Accessibility: hearing", "mono audio balance left right hearing 单声道 音频 平衡 左右 声道 听觉 无障碍";
    AccessibilityHearing, "Captions", "System / Accessibility: hearing", "captions subtitles text size style 字幕 听觉 字幕大小 字幕样式";
    AccessibilityVision, "Color inversion and correction", "System / Accessibility: colors", "accessibility vision color inversion correction grayscale colour 色彩 反转 颜色 校正 无障碍 灰度 色盲";
    DateTime, "Date and time", "System / Date and time", "date time timezone clock 日期 时间 时区 时钟";
    DateTime, "Set date and time", "System / Date and time", "manual date time calendar leap daylight saving 手动时间 设置日期 夏令时 日历";
    DateTime, "Choose time zone", "System / Date and time", "time zone region city utc timezone 时区 城市 地区";
    DateTime, "24-hour clock", "System / Date and time", "12 24 hour clock format 24小时 12小时 时间格式 小时制";
    DateTime, "Automatic time and time zone", "System / Date and time", "automatic network time zone 自动时间 自动时区 网络时间";
    About, "About phone", "About", "about phone device model manufacturer 关于手机 关于设备 型号 厂商";
    About, "Android and build version", "About", "android build version security patch kernel rom 版本 系统版本 安全补丁 内核";
    About, "Memory", "About", "memory ram available total 内存 运行内存 可用内存";
    Updates, "System updates", "System / System updates", "update updates ota rom upgrade download install restart reboot 系统更新 软件更新 升级 下载 重启";
    System, "System", "System", "system settings preferences 系统 设置 系统设置";
}

pub fn valid_query(query: &str) -> bool {
    query.chars().count() <= 128 && !query.chars().any(char::is_control)
}

fn normalize(value: &str) -> String {
    value.chars().flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Returns a small local inventory in stable relevance order. Paging is local;
/// neither the query nor matching aliases leave the Settings view.
pub fn search(query: &str) -> Vec<&'static SearchEntry> {
    if !valid_query(query) { return Vec::new(); }
    let query = normalize(query);
    if query.is_empty() { return Vec::new(); }
    let terms = query.split_whitespace().collect::<Vec<_>>();
    let mut matches = ENTRIES.iter().enumerate().filter_map(|(index, entry)| {
        let title = normalize(entry.title);
        let keywords = normalize(entry.keywords);
        let location = normalize(entry.location);
        let haystack = format!("{title} {keywords} {location}");
        if !terms.iter().all(|term| haystack.contains(term)) { return None; }
        let rank = if title == query { 0 } else if title.starts_with(&query) { 1 }
            else if title.contains(&query) { 2 }
            else if keywords.split_whitespace().any(|alias| alias == query) { 3 }
            else { 4 };
        Some((rank, index, entry))
    }).collect::<Vec<_>>();
    matches.sort_by_key(|(rank, index, _)| (*rank, *index));
    matches.into_iter().map(|(_, _, entry)| entry).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn english_and_chinese_aliases_select_reviewed_destinations() {
        for (query,route) in [("BRIGHTNESS",SettingsRoute::Display),("字体大小",SettingsRoute::Display),("自动同步",SettingsRoute::Accounts),("wi-fi",SettingsRoute::Wifi),("蓝牙",SettingsRoute::Bluetooth),("notification history",SettingsRoute::NotificationHistory),("省电",SettingsRoute::BatteryPolicy),("私人DNS",SettingsRoute::AdvancedNetwork),("airplane",SettingsRoute::AdvancedNetwork),("节省流量",SettingsRoute::AdvancedNetwork)] {
            assert!(search(query).iter().any(|entry|entry.route==route),"{query} should find {route:?}");
        }
        assert!(search("arbitrary.settings.intent://").is_empty());assert!(search("password reset factory erase").is_empty());
    }
    #[test]
    fn search_is_bounded_stable_and_rejects_control_text() {
        assert!(valid_query(&"字".repeat(128)));assert!(!valid_query(&"字".repeat(129)));
        for query in ["","   ","\nbrightness","volume\0"] {assert!(search(query).is_empty());}
        let first=search("a");assert!(first.len()>PAGE_SIZE&&first.len()<=ENTRIES.len());
        let second=search("A");assert_eq!(first.iter().map(|entry|entry.title).collect::<Vec<_>>(),second.iter().map(|entry|entry.title).collect::<Vec<_>>());
        assert_eq!(search("Brightness")[0].title,"Brightness");
    }
}
