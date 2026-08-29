//! 内置知识库 (skills) — 随应用打包的软件使用文档 + 系统提示词组装。
//!
//! 文档为 zh/en 双份 markdown (`skills/{zh,en}/*.md`,`include_str!` 编译期嵌入),
//! 跟随界面语言注入:系统提示词只放索引 (省 token),`read_skill` 工具按需读全文。

use std::fmt;

use error::{AiError, AppError, Result};

/// 单篇技能文档 (双语言内容在 [`SKILLS`] 里按 lang 取)。
pub struct Skill {
    /// 稳定 id (read_skill 参数 / 索引键)。
    pub id: &'static str,
    /// zh 标题 (索引用)。
    pub title_zh: &'static str,
    /// en 标题 (索引用)。
    pub title_en: &'static str,
    /// zh 一句话用途。
    pub summary_zh: &'static str,
    /// en 一句话用途。
    pub summary_en: &'static str,
    /// zh 全文。
    pub md_zh: &'static str,
    /// en 全文。
    pub md_en: &'static str,
}

/// 界面语言 (前端 appStore.lang 同构)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    /// 解析语言字符串,未知值回退中文。
    pub fn parse(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("en") {
            Self::En
        } else {
            Self::Zh
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Zh => "zh",
            Self::En => "en",
        })
    }
}

/// 全部技能文档 (id 稳定,新增追加)。
pub const SKILLS: &[Skill] = &[
    Skill {
        id: "overview",
        title_zh: "软件与核心概念总览",
        title_en: "Software & Core Concepts Overview",
        summary_zh: "软件定位、字节/数值双平面、tab 与全局节点、数据流",
        summary_en: "App purpose, byte/value planes, tabs & global nodes, data flow",
        md_zh: include_str!("../skills/zh/overview.md"),
        md_en: include_str!("../skills/en/overview.md"),
    },
    Skill {
        id: "nodes-reference",
        title_zh: "节点与控件类型参考",
        title_en: "Node & Widget Type Reference",
        summary_zh: "传输/协议类型与参数、全部 widget、连线规则与端口域 (同域才能连)",
        summary_en: "Transport/protocol kinds & params, all widgets, wiring rules & port domains",
        md_zh: include_str!("../skills/zh/nodes-reference.md"),
        md_en: include_str!("../skills/en/nodes-reference.md"),
    },
    Skill {
        id: "protocols",
        title_zh: "协议与数据格式",
        title_en: "Protocols & Data Formats",
        summary_zh: "JustFloat/FireWater 帧格式、自定义帧、CAN、逻辑解码",
        summary_en: "JustFloat/FireWater frames, custom frames, CAN, logic decode",
        md_zh: include_str!("../skills/zh/protocols.md"),
        md_en: include_str!("../skills/en/protocols.md"),
    },
    Skill {
        id: "debug-recipes",
        title_zh: "设备调试实战手册",
        title_en: "Device Debugging Playbook",
        summary_zh: "连接设备→解析→绘图、无硬件调试、无数据排查、CAN 调试等任务套路",
        summary_en: "Connect→parse→plot, hardware-less debug, no-data triage, CAN",
        md_zh: include_str!("../skills/zh/debug-recipes.md"),
        md_en: include_str!("../skills/en/debug-recipes.md"),
    },
    Skill {
        id: "tools-guide",
        title_zh: "内置工具使用指南",
        title_en: "Built-in Tools Guide",
        summary_zh: "工具清单、先读后写工作流、上限与注意事项",
        summary_en: "Tool inventory, read-before-write workflow, caps & caveats",
        md_zh: include_str!("../skills/zh/tools-guide.md"),
        md_en: include_str!("../skills/en/tools-guide.md"),
    },
];

/// 读取技能全文;id 不存在返回错误。
///
/// # Errors
/// 未知 skill_id 返回 [`AiError::SkillNotFound`]。
pub fn read_skill(skill_id: &str, lang: Lang) -> Result<String> {
    let skill = SKILLS.iter().find(|s| s.id == skill_id).ok_or_else(|| {
        AppError::Ai(AiError::SkillNotFound {
            skill: skill_id.to_string(),
        })
    })?;
    let md = match lang {
        Lang::Zh => skill.md_zh,
        Lang::En => skill.md_en,
    };
    Ok(md.to_string())
}

/// 组装启用内置工具时的系统提示词:基础约定 + 知识库索引 + 用户自填提示词。
pub fn compose_system_prompt(lang: Lang, user: Option<&str>) -> String {
    let mut out = match lang {
        Lang::Zh => String::from(
            "你是 VOFA-NEXT (串口/CAN/波形调试上位机) 的内置 AI 助手,可以调用内置工具\n\
             编辑节点图、操作软件、读取设备数据。工作约定:\n\
             - 编辑节点前先用 get_workspace 读当前画布状态;节点 id 一律来自读取结果,不要捏造。\n\
             - 小步修改:每做一次编辑,用对应读取工具 (get_graph_outputs / get_recent_waveform /\n\
               get_can_frames 等) 验证效果后再继续。\n\
             - 设备无数据时按序排查:连接状态 → get_raw_data 原始字节 → 连线与协议配置。\n\
             - 用中文与用户交流 (除非用户使用其他语言)。\n",
        ),
        Lang::En => String::from(
            "You are the built-in AI assistant of VOFA-NEXT (a serial/CAN/waveform debugging\n\
             host app). You can call built-in tools to edit the node graph, operate the app,\n\
             and read device data. Working conventions:\n\
             - Call get_workspace before editing the graph; only use node ids from read results.\n\
             - Edit in small steps: verify each edit with the matching read tool\n\
               (get_graph_outputs / get_recent_waveform / get_can_frames ...) before continuing.\n\
             - For \"no data\", triage in order: connection state → get_raw_data raw bytes →\n\
               wiring and protocol config.\n\
             - Reply in the user's language.\n",
        ),
    };

    // 知识库索引 (标题 + 一句话用途, 全文按需 read_skill)
    out.push_str(match lang {
        Lang::Zh => "\n可用知识库 (read_skill 工具按需读取全文):\n",
        Lang::En => "\nKnowledge base (read full text via the read_skill tool):\n",
    });
    for s in SKILLS {
        match lang {
            Lang::Zh => out.push_str(&format!("- {}: {} — {}\n", s.id, s.title_zh, s.summary_zh)),
            Lang::En => out.push_str(&format!("- {}: {} — {}\n", s.id, s.title_en, s.summary_en)),
        }
    }

    if let Some(user) = user.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str(match lang {
            Lang::Zh => "\n---\n用户自定义补充指令:\n",
            Lang::En => "\n---\nUser-provided additional instructions:\n",
        });
        out.push_str(user);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部文档可读,中英内容非空且 id 唯一。
    #[test]
    fn skills_are_complete_and_unique() {
        let mut ids = std::collections::HashSet::new();
        for s in SKILLS {
            assert!(ids.insert(s.id), "重复 skill id: {}", s.id);
            assert!(s.md_zh.len() > 200, "{} zh 文档过短", s.id);
            assert!(s.md_en.len() > 200, "{} en 文档过短", s.id);
            assert!(read_skill(s.id, Lang::Zh).is_ok());
            assert!(read_skill(s.id, Lang::En).is_ok());
        }
        assert!(SKILLS.len() >= 5);
    }

    /// 未知 id 报 SkillNotFound。
    #[test]
    fn unknown_skill_errors() {
        let err = read_skill("nope", Lang::Zh).unwrap_err();
        assert!(err.to_string().contains("nope"));
    }

    /// 系统提示词: 含约定 + 索引;用户提示词追加在后;En 分支英文索引。
    #[test]
    fn system_prompt_composition() {
        let zh = compose_system_prompt(Lang::Zh, Some("  自定义:波特率默认 9600  "));
        assert!(zh.contains("get_workspace"));
        assert!(zh.contains("- overview:"));
        assert!(zh.contains("自定义:波特率默认 9600"));
        assert!(zh.find("自定义").unwrap() > zh.find("- overview:").unwrap());

        let en = compose_system_prompt(Lang::En, None);
        assert!(en.contains("Knowledge base"));
        assert!(!en.contains("用户自定义"));
    }

    /// 语言解析: 未知回退中文。
    #[test]
    fn lang_parse_fallback() {
        assert_eq!(Lang::parse("en"), Lang::En);
        assert_eq!(Lang::parse("EN"), Lang::En);
        assert_eq!(Lang::parse("zh"), Lang::Zh);
        assert_eq!(Lang::parse("fr"), Lang::Zh);
    }
}
