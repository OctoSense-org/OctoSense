//! Cheap, deterministic topic relevance: does a headline or an article
//! mention a search's topic?
//!
//! It lives on the host side, not in a backend, so it applies to whatever
//! [`super::ResearchBackend`] finds and reads (the interim adapter today, the
//! octos research engine later).
//!
//! The topic is split into terms; stop-words ("of", "the", "news", "de",
//! "新闻" …) and one-letter words are ignored. Words in scripts written with
//! spaces (Latin, Cyrillic, Greek …) match **whole words**, case- and
//! accent-insensitively, with a light suffix stemmer (`islands` ~ `island`,
//! `resigns` ~ `resignation` ~ `resigned`). Runs of Han, kana, Hangul or
//! Thai match as **substrings**, after both sides are folded to Simplified
//! Chinese with a small character table, so a Simplified term matches a
//! Traditional article and vice versa. The table covers common news
//! vocabulary, not all of Chinese: a term with a character outside it
//! matches only the same script.
//!
//! Two tests:
//!
//! - [`Topic::matches`], for a headline and its summary: **every** term must
//!   appear (so the whole phrase is covered whenever the terms are);
//! - [`Topic::mentioned_in`], for an article's text, which is judged on
//!   the topic's **names** (words written with a capital or a digit):
//!   every short name (under four letters: `EU`, `AI`, `Act`, `US`) must
//!   appear, and at least one of the longer ones (`Hormuz`, `Nvidia`,
//!   `Northwind`) when there are any, since a query often stacks a name
//!   with its aliases (`Northwind Semiconductors NRTH`). A topic without
//!   names needs at least a third of its terms (rounded up). A run of four
//!   or more unspaced characters also counts when its first or last half
//!   appears (`河流清理` ~ `河道清理`), since such a run cannot be split
//!   into words. This is a cheap floor against plainly off-topic pages; the
//!   digest's model check catches subtler ones (`Taiwan Strait` passes the
//!   floor for `Strait of Hormuz`).
//!
//! A topic with no significant term (all stop-words) matches everything.

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

/// A topic's significant terms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Topic {
    /// Stemmed, folded whole words.
    words: Vec<String>,
    /// The words written with a capital or a digit in the topic (stemmed),
    /// and whether each is short (under four letters as written).
    names: Vec<(String, bool)>,
    /// Folded runs of scripts written without spaces, matched as substrings.
    runs: Vec<String>,
}

impl Topic {
    pub fn new(topic: &str) -> Topic {
        let mut words = Vec::new();
        let mut names = Vec::new();
        let mut runs = Vec::new();
        for token in tokens(topic) {
            match token {
                Token::Word(w, name) => {
                    if w.chars().count() < 2 || STOP_WORDS.contains(&w.as_str()) {
                        continue;
                    }
                    let stem = stem(&w);
                    if name && !names.iter().any(|(n, _)| *n == stem) {
                        names.push((stem.clone(), w.chars().count() < 4));
                    }
                    if !words.contains(&stem) {
                        words.push(stem);
                    }
                }
                Token::Run(r) => {
                    let mut r = r;
                    for filler in CJK_FILLERS {
                        r = r.replace(&fold_cjk(filler), "");
                    }
                    if !r.is_empty() && !runs.contains(&r) {
                        runs.push(r);
                    }
                }
            }
        }
        Topic { words, names, runs }
    }

    /// No significant term: everything matches.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.runs.is_empty()
    }

    /// The significant terms, for diagnostics.
    pub fn terms(&self) -> Vec<String> {
        self.words.iter().chain(&self.runs).cloned().collect()
    }

    /// Whether `text` (a headline and summary) mentions every term.
    pub fn matches(&self, text: &str) -> bool {
        if self.is_empty() {
            return true;
        }
        let text = Indexed::new(text);
        self.words.iter().all(|w| text.words.contains(w))
            && self.runs.iter().all(|r| text.runs.contains(r.as_str()))
    }

    /// Whether `text` (an article) mentions the topic: every short name
    /// and one of the longer names; without names, a third of the terms.
    pub fn mentioned_in(&self, text: &str) -> bool {
        if self.is_empty() {
            return true;
        }
        let text = Indexed::new(text);
        if !self.names.is_empty() {
            let has = |n: &&(String, bool)| text.words.contains(&n.0);
            let mut long = self.names.iter().filter(|n| !n.1).peekable();
            return self.names.iter().filter(|n| n.1).all(|n| has(&n))
                && (long.peek().is_none() || long.any(|n| has(&n)));
        }
        let found = self
            .words
            .iter()
            .filter(|w| text.words.contains(*w))
            .count()
            + self.runs.iter().filter(|r| text.mentions_run(r)).count();
        let terms = self.words.len() + self.runs.len();
        found * 3 >= terms
    }

    /// Whether `text` mentions any of `topics` (an item a run found through
    /// several searches is relevant if it is relevant to one of them).
    pub fn mentioned_in_any(topics: &[Topic], text: &str) -> bool {
        topics.is_empty() || topics.iter().any(|t| t.mentioned_in(text))
    }
}

/// A text's stemmed words and its unspaced runs, folded.
struct Indexed {
    words: BTreeSet<String>,
    /// Runs separated by spaces, so a term cannot span two of them.
    runs: String,
}

impl Indexed {
    fn new(text: &str) -> Indexed {
        let mut words = BTreeSet::new();
        let mut runs = String::new();
        for token in tokens(text) {
            match token {
                Token::Word(w, _) => {
                    words.insert(stem(&w));
                }
                Token::Run(r) => {
                    runs.push_str(&r);
                    runs.push(' ');
                }
            }
        }
        Indexed { words, runs }
    }

    /// The whole run, or for four characters or more, its first or last
    /// half.
    fn mentions_run(&self, run: &str) -> bool {
        if self.runs.contains(run) {
            return true;
        }
        let chars: Vec<char> = run.chars().collect();
        if chars.len() < 4 {
            return false;
        }
        let half = chars.len() / 2;
        let head: String = chars[..half].iter().collect();
        let tail: String = chars[chars.len() - half..].iter().collect();
        self.runs.contains(&head) || self.runs.contains(&tail)
    }
}

enum Token {
    /// A folded, lowercased word, and whether it was written with a capital
    /// or a digit.
    Word(String, bool),
    Run(String),
}

/// Scripts written without spaces between words.
fn unspaced(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // hiragana, katakana
        | 0x3400..=0x4DBF // CJK extension A
        | 0x4E00..=0x9FFF // CJK unified ideographs
        | 0xF900..=0xFAFF // CJK compatibility ideographs
        | 0xAC00..=0xD7AF // Hangul syllables
        | 0x0E00..=0x0E7F // Thai
    )
}

/// Words (folded, lowercased) and unspaced runs (folded to Simplified).
fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut name = false;
    let mut run = String::new();
    for c in text.chars() {
        let spaced = !unspaced(c) && c.is_alphanumeric();
        if !spaced && !word.is_empty() {
            out.push(Token::Word(std::mem::take(&mut word), name));
            name = false;
        }
        if !unspaced(c) && !run.is_empty() {
            out.push(Token::Run(std::mem::take(&mut run)));
        }
        if unspaced(c) {
            run.push(to_simplified(c));
        } else if spaced {
            name |= c.is_uppercase() || c.is_numeric();
            for lower in c.to_lowercase() {
                push_folded(lower, &mut word);
            }
        }
    }
    if !word.is_empty() {
        out.push(Token::Word(word, name));
    }
    if !run.is_empty() {
        out.push(Token::Run(run));
    }
    out
}

/// A light suffix stemmer for whole-word matching. Both sides go through
/// it, so it only has to be consistent, not linguistically right.
fn stem(word: &str) -> String {
    let mut w = word.to_owned();
    for (suffix, replacement) in [
        ("ations", ""),
        ("ation", ""),
        ("ings", ""),
        ("ing", ""),
        ("ies", "y"),
        ("ied", "y"),
        ("ed", ""),
    ] {
        if let Some(base) = w.strip_suffix(suffix) {
            if base.chars().count() >= 3 {
                w = format!("{base}{replacement}");
                break;
            }
        }
    }
    if w.chars().count() > 3 && w.ends_with('s') && !w.ends_with("ss") {
        w.pop();
    }
    if w.chars().count() > 3 && w.ends_with('e') {
        w.pop();
    }
    w
}

/// Pushes `c` with common Latin accents folded (`č` → `c`, `ß` → `ss`).
fn push_folded(c: char, out: &mut String) {
    const TABLE: &[(&str, &str)] = &[
        ("àáâãäåāăą", "a"),
        ("çćĉċč", "c"),
        ("ďđ", "d"),
        ("èéêëēĕėęě", "e"),
        ("ĝğġģ", "g"),
        ("ĥħ", "h"),
        ("ìíîïĩīĭįı", "i"),
        ("ĵ", "j"),
        ("ķ", "k"),
        ("ĺļľŀł", "l"),
        ("ñńņňŉ", "n"),
        ("òóôõöøōŏő", "o"),
        ("ŕŗř", "r"),
        ("śŝşšș", "s"),
        ("ţťŧț", "t"),
        ("ùúûüũūŭůűų", "u"),
        ("ŵ", "w"),
        ("ýÿŷ", "y"),
        ("źżž", "z"),
        ("ß", "ss"),
        ("æ", "ae"),
        ("œ", "oe"),
    ];
    match TABLE.iter().find(|(from, _)| from.contains(c)) {
        Some((_, to)) => out.push_str(to),
        None => out.push(c),
    }
}

/// Folds a string of unspaced script to Simplified.
fn fold_cjk(text: &str) -> String {
    text.chars().map(to_simplified).collect()
}

fn to_simplified(c: char) -> char {
    static MAP: OnceLock<HashMap<char, char>> = OnceLock::new();
    *MAP.get_or_init(|| {
        TRAD_SIMP
            .split_whitespace()
            .filter_map(|pair| {
                let mut chars = pair.chars();
                Some((chars.next()?, chars.next()?))
            })
            .collect()
    })
    .get(&c)
    .unwrap_or(&c)
}

/// Stop-words ignored in topics: function words in the languages the
/// toolbox searches most, and words that describe news rather than a topic.
const STOP_WORDS: &[&str] = &[
    // English
    "a",
    "an",
    "the",
    "of",
    "in",
    "on",
    "at",
    "to",
    "for",
    "from",
    "by",
    "with",
    "and",
    "or",
    "is",
    "are",
    "was",
    "be",
    "as",
    "its",
    "it",
    "this",
    "that",
    "about",
    "into",
    "over",
    "vs",
    "news",
    "latest",
    "update",
    "updates",
    "today",
    "new",
    // Spanish, Portuguese, French, Italian, German
    "de",
    "del",
    "la",
    "las",
    "el",
    "los",
    "y",
    "en",
    "un",
    "una",
    "por",
    "para",
    "con",
    "da",
    "do",
    "das",
    "dos",
    "e",
    "o",
    "os",
    "le",
    "les",
    "des",
    "du",
    "et",
    "au",
    "aux",
    "di",
    "il",
    "lo",
    "gli",
    "della",
    "der",
    "die",
    "das",
    "und",
    "im",
    "von",
    "zu",
    "den",
    "dem",
    "noticias",
    "nouvelles",
    "actualités",
    "nachrichten",
    "notizie",
];

/// Filler words in Chinese and Japanese topics (after translation a model
/// may add them).
const CJK_FILLERS: &[&str] = &[
    "最新消息",
    "最新动态",
    "最新",
    "新闻",
    "消息",
    "报道",
    "动态",
    "近况",
    "ニュース",
];

/// Traditional → Simplified pairs for common news vocabulary, one pair per
/// token. Both sides of a match are folded, so a pair only has to be
/// consistent.
const TRAD_SIMP: &str = "
颱台 臺台 風风 灣湾 國国 華华 們们 這这 個个 來来 說说 會会 經经 發发 對对 為为 與与 從从 還还 時时
後后 過过 開开 關关 長长 門门 問问 間间 聞闻 題题 業业 產产 東东 車车 軍军 戰战 爭争 議议 選选 舉举
總总 統统 權权 黨党 務务 當当 區区 將将 報报 導导 據据 無无 認认 點点 動动 員员 機机 構构 決决 應应
實实 現现 頭头 條条 萬万 億亿 價价 錢钱 銀银 貿贸 稅税 資资 購购 買买 賣卖 場场 氣气 溫温 雲云 災灾
颶飓 預预 衛卫 醫医 療疗 藥药 險险 級级 號号 衝冲 擊击 傷伤 難难 準准 備备 擬拟 協协 談谈 論论 調调
證证 設设 計计 劃划 術术 網网 絡络 電电 腦脑 話话 訊讯 視视 聽听 讀读 書书 寫写 學学 習习 師师 輸输
運运 鐵铁 飛飞 艦舰 彈弹 槍枪 砲炮 邊边 領领 義义 歐欧 羅罗 烏乌 蘭兰 韓韩 紐纽 約约 倫伦 頓顿 爾尔
茲兹 峽峡 維维 辭辞 職职 輝辉 達达 財财 體体 規规 範范 監监 蘋苹 訓训 練练 語语 狀状 態态 況况 顯显
圖图 館馆 樂乐 觀观 環环 護护 燒烧 熱热 雙双 貨货 幣币 匯汇 漲涨 盤盘 債债 虧亏 損损 營营 額额 單单
圍围 鄉乡 農农 糧粮 豐丰 陸陆 島岛 嶼屿 廣广 蘇苏 滬沪 淪沦 擴扩 張张 縮缩 減减 屬属 於于 並并 麼么
裡里 著着 讓让 給给 幾几 兩两 樣样 種种 麗丽 歲岁 圓圆 團团 質质 積积 極极 標标 則则 結结 係系 繫系
聯联 絕绝 續续 紀纪 錄录 織织 綠绿 紅红 線线 細细 終终 組组 紙纸 納纳 綱纲 緊紧 編编 縣县 繳缴 繼继
變变 讚赞 譯译 識识 詞词 試试 誤误 請请 諾诺 謂谓 謝谢 講讲 記记 許许 評评 詳详 誠诚 誰谁 課课 諸诸
謀谋 謊谎 豬猪 貓猫 貝贝 負负 責责 貧贫 販贩 貪贪 費费 賀贺 賠赔 賭赌 賽赛 贏赢 趙赵 趕赶 軌轨 軟软
輕轻 較较 載载 輛辆 輪轮 轉转 辦办 遠远 違违 連连 進进 遊游 遲迟 遺遗 鄰邻 釋释 針针 鈔钞 鋼钢 鍵键
鎮镇 鏡镜 閉闭 閃闪 閱阅 闊阔 隊队 階阶 際际 陳陈 陽阳 陰阴 隨随 雖虽 雜杂 雞鸡 離离 霧雾 靈灵 頁页
項项 順顺 須须 頻频 顆颗 顏颜 願愿 類类 顧顾 飯饭 飲饮 馬马 駐驻 驗验 騰腾 驚惊 髮发 鬥斗 魚鱼 鮮鲜
鳥鸟 鳴鸣 麥麦 黃黄 齊齐 齒齿 龍龙 龜龟 傳传 僅仅 優优 儲储 兒儿 內内 冊册 凍冻 劇剧 劍剑 勞劳 勢势
勝胜 厲厉 參参 嚴严 園园 執执 堅坚 塊块 塵尘 壓压 壞坏 壯壮 聲声 處处 復复 夠够 夢梦 奮奋 奪夺 婦妇
媽妈 孫孙 寧宁 審审 寬宽 寶宝 專专 尋寻 層层 崗岗 帶带 帳帐 幫帮 廳厅 強强 歸归 徑径 徵征 恆恒 惡恶
愛爱 慣惯 憂忧 懷怀 戲戏 戶户 拋抛 挾挟 捨舍 掃扫 掛挂 採采 換换 揚扬 搖摇 擁拥 擇择 擔担 擺摆 攝摄
敗败 敵敌 數数 斷断 晉晋 暫暂 曆历 歷历 樓楼 橋桥 歡欢 殘残 殺杀 殼壳 漢汉 滅灭 滿满 漁渔 潔洁 濟济
濕湿 煙烟 燈灯 爐炉 牆墙 犧牺 獎奖 獨独 獲获 獻献 畫画 疊叠 盡尽 眾众 確确 碼码 礎础 禮礼 禍祸 稱称
穩稳 窮穷 競竞 筆笔 節节 築筑 簡简 絲丝 綜综 緒绪 緣缘 縱纵 繩绳 罰罚 罷罢 聖圣 聰聪 肅肃 脅胁 腳脚
臉脸 臨临 興兴 舊旧 艙舱 藝艺 蟲虫 補补 裝装 製制 複复 見见 親亲 覺觉 覽览 觸触 訂订 討讨 訪访 訴诉
診诊 註注 詐诈 該该 誌志 諮咨 譜谱 豈岂 賴赖 贊赞 趨趋 跡迹 踐践 蹤踪 輔辅 輯辑 轄辖 遞递 適适 遷迁
郵邮 鄭郑 釀酿 鈴铃 銷销 鋒锋 錯错 鍋锅 鎖锁 鐘钟 閣阁 隱隐 靜静 響响 頂顶 飢饥 驅驱 鬧闹 齡龄 颮飑
潰溃 攜携 搶抢 賑赈 衆众
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_pairs() {
        let mut seen = std::collections::BTreeSet::new();
        for pair in TRAD_SIMP.split_whitespace() {
            let chars: Vec<char> = pair.chars().collect();
            assert_eq!(chars.len(), 2, "{pair}");
            assert_ne!(chars[0], chars[1], "{pair}");
            assert!(seen.insert(chars[0]), "{pair} repeats");
        }
    }

    #[test]
    fn stop_words_are_ignored_and_every_term_is_required() {
        let hormuz = Topic::new("Strait of Hormuz");
        assert_eq!(hormuz.terms(), vec!["strait", "hormuz"]);
        assert!(hormuz.matches("Tankers wait at the Strait of Hormuz"));
        assert!(!hormuz.matches("Pope Leo warns of a paradise of machines"));
        assert!(
            !hormuz.matches("Iran's straits and narrows"),
            "hormuz missing"
        );
        let act = Topic::new("EU AI Act");
        assert!(act.matches("The EU's AI Act enters its second phase"));
        assert!(!act.matches("OpenAI halts training after AI agents act up"));
        assert!(!act.matches("Pope Leo on AI"));
        assert!(Topic::new("the news of").is_empty());
        assert!(Topic::new("the news of").matches("anything"));
    }

    #[test]
    fn whole_words_not_substrings() {
        let ai = Topic::new("AI");
        assert!(ai.matches("New AI rules"));
        assert!(!ai.matches("Said the chair"), "ai inside said/chair");
        assert!(!Topic::new("act").matches("Impact on actors"));
    }

    #[test]
    fn inflections_and_accents() {
        assert!(Topic::new("urban heat islands").matches("An island of heat in urban areas"));
        let vucic = Topic::new("Vucic resignation");
        assert!(vucic.matches("Serbia's Vučić resigns ahead of elections"));
        assert!(vucic.matches("Vučić resigned on Friday"));
        assert!(!vucic.matches("Vučić holds rally"));
        assert!(Topic::new("Nvidia earnings").matches("Nvidia's earning call"));
        assert!(Topic::new("OpenAI").matches("OpenAI's new model"));
    }

    #[test]
    fn an_article_must_mention_every_name() {
        let hormuz = Topic::new("Strait of Hormuz");
        assert!(hormuz.mentioned_in("Iran said tankers may pass the Strait of Hormuz."));
        assert!(!hormuz.mentioned_in("OpenAI pauses training; the Pope speaks on AI."));
        let act = Topic::new("EU AI Act");
        assert!(act.mentioned_in("Brussels: the EU's AI Act applies from August."));
        assert!(!act.mentioned_in("Pope Leo warns about AI; regulators must act."));
        // One of several long names is enough.
        let northwind = Topic::new("Northwind Semiconductors NRTH stock");
        assert!(northwind.mentioned_in("Northwind reported record orders."));
        assert!(!northwind.mentioned_in("Contoso Chips faces an export review."));
        // Descriptive words need not appear when the names do.
        let nvidia = Topic::new("Nvidia earnings");
        assert!(nvidia.mentioned_in("Nvidia shares rose after quarterly results."));
        assert!(!nvidia.mentioned_in("Intel's best years may be ahead, says history."));
        assert!(Topic::new("Contoso Chips CNTS stock").mentioned_in("Contoso said on Monday"));
        // No names: at least a third of the terms.
        let cleanup = Topic::new("river cleanup");
        assert!(cleanup.mentioned_in("Sensors will track the river's pollution."));
        assert!(!cleanup.mentioned_in("A new stadium opens downtown."));
        // A long unspaced run: its first or last half is enough.
        let zh = Topic::new("河流清理");
        assert!(zh.mentioned_in("河道清理完成后，鱼类回归。"));
        assert!(!zh.mentioned_in("台风登陆广东。"));
        assert!(Topic::new("台风").mentioned_in("超強颱風登陸"));
        assert!(!Topic::new("霍尔木兹海峡").mentioned_in("台湾海峡局势"));
    }

    #[test]
    fn cjk_is_a_substring_match_across_scripts() {
        let typhoon = Topic::new("台风");
        assert!(typhoon.matches("超强台风登陆广东"));
        // BBC 中文's "simp" feed is served in Traditional script.
        assert!(typhoon.matches("超強颱風登陸廣東"));
        assert!(!typhoon.matches("暴雨袭击北京"));
        let hormuz = Topic::new("霍尔木兹海峡");
        assert!(hormuz.matches("伊朗在霍爾木茲海峽扣押油輪"));
        // Filler words a translation may add are ignored.
        let openai = Topic::new("OpenAI 最新消息");
        assert_eq!(openai.terms(), vec!["openai"]);
        assert!(openai.matches("OpenAI暂停训练"));
        // Mixed: both the Latin word and the Han run are required.
        let mixed = Topic::new("OpenAI 训练");
        assert!(mixed.matches("OpenAI 暫停訓練"));
        assert!(!mixed.matches("OpenAI 发布新模型"));
    }
}
