// workspace 的寻址层(C2 定案的模块本体):TargetRef 是复合地址的唯一定义,
// ResolvedManagerConfig 的五个寻址入口(target / target_by_id / target_refs /
// managed_refs / skip_reason)与落点归属(skill_dir_links)只在这里。types.rs
// 只留数据形状;「地址怎么解析、谁参与分发」的判定不与 DTO 同居。

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;

use super::types::{AgentTargetId, ResolvedManagerConfig, ResolvedTargetConfig};

// 落点链接的探测结果:链接另一端是受管 target 时的 id、解出的真实目录
// (不是链接时 None)。复合地址由返回的三元组携带,不在结构体里重复。
pub(super) struct SkillDirLink {
    // 链接的另一端是某个受管 target 时给出它的 id；外部目录/断链时为 None。
    pub(super) linked_target_id: Option<String>,
    // 落点不是链接时为 None。
    pub(super) link_target: Option<PathBuf>,
}

impl SkillDirLink {
    pub(super) fn is_alias(&self) -> bool {
        self.link_target.is_some()
    }
}

// 目标不参与分发的原因。调用方只决定表现方式（报错 / 提示 / 静默），
// 「为什么不参与」由这里回答。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TargetSkip {
    // 地址不存在：输入来自客户端（弹窗快照、命令参数）时才会出现。
    Missing,
    // skill_dir 整层是一个目录链接（指向另一个受管 target、外部目录或已断链）：
    // 内容由链接的另一端负责，本 target 不参与安装与清理。
    LinkedDir,
}

// 一个 target 的地址：全局 target 直接用 id；项目级 target 需要 (项目, agent)
// 两段。复合 id 的**构造与拆分都只在这里**（此前 3 处组串、1 处内联拆分，
// 「格式」没有单一所有者）。
//
// parse 只服务「外部只给了字符串」的入口（命令参数、config.yaml 手写值）：
// 内部已有 TargetRef 的地方一律直接用 TargetRef，不再绕回字符串。
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(super) enum TargetRef {
    Global(AgentTargetId),
    Project {
        project_id: String,
        agent_id: AgentTargetId,
    },
}

impl TargetRef {
    // 面向 UI 与命令的稳定字符串：全局 = id，项目级 = `project:agent`。
    pub(super) fn composite_id(&self) -> String {
        match self {
            TargetRef::Global(id) => id.0.clone(),
            TargetRef::Project {
                project_id,
                agent_id,
            } => format!("{project_id}:{}", agent_id.0),
        }
    }

    // `project:agent` 拆回两段；不含分隔符时按全局 id 处理。两段原样保留，
    // 不做规范化：查不到就是查不到，不能悄悄映射到另一个 target。
    pub(super) fn parse(composite_id: &str) -> TargetRef {
        match composite_id.split_once(':') {
            Some((project_id, agent_id)) if !project_id.is_empty() && !agent_id.is_empty() => {
                TargetRef::Project {
                    project_id: project_id.to_string(),
                    agent_id: AgentTargetId(agent_id.to_string()),
                }
            }
            _ => TargetRef::Global(AgentTargetId(composite_id.to_string())),
        }
    }
}

// 「target 不可用」的唯一说法：调用方只需决定「报错还是记一条 warning」，
// 不必各自措辞（此前同一个故障有三种文案、两种语言）。
pub(super) fn target_unavailable(composite_id: &str) -> anyhow::Error {
    anyhow::anyhow!("target 不存在：{composite_id}")
}

impl ResolvedManagerConfig {
    // 唯一的寻址入口。调用方拿到的是配置里的真实 target，不是重新拼出来的。
    pub(super) fn target(&self, reference: &TargetRef) -> Option<&ResolvedTargetConfig> {
        match reference {
            TargetRef::Global(id) => self.targets.get(id),
            TargetRef::Project {
                project_id,
                agent_id,
            } => self.projects.get(project_id)?.agents.get(agent_id),
        }
    }

    // 复合 id 直达寻址；不存在返回 None。
    pub(super) fn target_by_id(&self, composite_id: &str) -> Option<&ResolvedTargetConfig> {
        self.target(&TargetRef::parse(composite_id))
    }

    // 全部 target 及其地址，顺序为「全局在前、项目按配置顺序」。枚举与批量操作
    // 都从这里取；需要展示串时再调 composite_id，拆串只发生在 TargetRef::parse。
    pub(super) fn target_refs(&self) -> Vec<(TargetRef, &ResolvedTargetConfig)> {
        let global = self
            .targets
            .values()
            .map(|target| (TargetRef::Global(target.id.clone()), target));
        let project = self.projects.values().flat_map(|project| {
            project.agents.values().map(move |target| {
                (
                    TargetRef::Project {
                        project_id: project.id.clone(),
                        agent_id: target.id.clone(),
                    },
                    target,
                )
            })
        });
        global.chain(project).collect()
    }

    // 该 target 是否参与技能分发。判定只有这一处；调用方只决定「跳过时怎么表现」
    // （报错 / warning / 静默）。MCP 分发与状态展示不经过它——那条链路问的是
    // config_path 而不是 skill_dir。
    //
    // 为什么放在寻址层而不是安装面：它回答的是「这个地址可用吗」，
    // 是对 TargetRef 的补充，而不是技能安装的私有规则。
    pub(super) fn skip_reason(&self, reference: &TargetRef) -> Result<Option<TargetSkip>> {
        let Some(target) = self.target(reference) else {
            return Ok(Some(TargetSkip::Missing));
        };
        // 项目级 target 的 skill_dir 整层指向别处时，内容由链接的另一端负责，
        // 这里不自作主张。
        //
        // 注意：这里只看「是不是链接」，不看「指向谁」；与展示层的
        // `linked_target_id`（只看指向另一个受管 target、且对全局 target 也生效）
        // 并不等价，两处不对称是既有行为，本次重构原样保留，
        // 见 background: 前后端判定不一致待单独立项。
        if target.is_project && super::targets::is_linked_skill_dir(&target.skill_dir)? {
            return Ok(Some(TargetSkip::LinkedDir));
        }
        Ok(None)
    }

    // 全部 target 的落点链接状态（只读探测）:(地址, target, 链接状态) 三元组
    // 一次给出,调用方不再各自把两条独立序列配对。落点归属与参与判定都从
    // 这里取，不给调用方各自 canonicalize 的机会——落点是不是链接、内容属于
    // 谁、解出来的真实目录，答案只有一处。
    pub(super) fn skill_dir_links(
        &self,
    ) -> Result<Vec<(TargetRef, &ResolvedTargetConfig, SkillDirLink)>> {
        // 受管落点 → target id。归属比较按「链接自身路径」算：对目录链接
        // canonicalize 会解析到另一端，别名就会与它的目标撞进同一个 key。
        let mut dir_to_target: HashMap<PathBuf, String> = HashMap::new();
        for (reference, target) in self.target_refs() {
            dir_to_target
                .entry(super::skill_install::normalize_own_dir(&target.skill_dir))
                .or_insert_with(|| reference.composite_id());
        }

        self.target_refs()
            .into_iter()
            .map(|(reference, target)| {
                let linked = super::targets::read_skill_dir_link(&target.skill_dir)?;
                let link = SkillDirLink {
                    linked_target_id: linked
                        .as_ref()
                        .and_then(|link_target| dir_to_target.get(link_target).cloned()),
                    link_target: linked,
                };
                Ok((reference, target, link))
            })
            .collect()
    }

    // 全部 target 的 (地址, target, 是否参与分发)。
    pub(super) fn managed_refs(
        &self,
    ) -> Result<Vec<(TargetRef, &ResolvedTargetConfig, Option<TargetSkip>)>> {
        self.target_refs()
            .into_iter()
            .map(|(reference, target)| {
                let skip = self.skip_reason(&reference)?;
                Ok((reference, target, skip))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::TargetRef;
    use crate::workspace::types::AgentTargetId;

    // 复合 id 的往返与边界：格式只有这一处所有者，行为必须有直测守着。
    #[test]
    fn target_ref_round_trips_through_composite_id() {
        let global = TargetRef::Global(AgentTargetId("codex".to_string()));
        assert_eq!(global.composite_id(), "codex");
        assert_eq!(TargetRef::parse(&global.composite_id()), global);

        let project = TargetRef::Project {
            project_id: "my-app".to_string(),
            agent_id: AgentTargetId("claude".to_string()),
        };
        assert_eq!(project.composite_id(), "my-app:claude");
        assert_eq!(TargetRef::parse(&project.composite_id()), project);
    }

    // 空段一律按全局 id 处理（`:codex` / `demo:` / `:` 都不是合法项目地址）；
    // 多冒号归第一段——项目 id 含 `:` 时这个地址必然查不到，读取侧会明确报错。
    #[test]
    fn target_ref_rejects_empty_segments() {
        // 与前端 model.test.ts::parseTargetId 的同一用例集：两端口径必须一致，
        // 否则同一个 id 在前后端会落到不同项目分组。
        for malformed in [":codex", "demo:", ":"] {
            assert!(
                matches!(TargetRef::parse(malformed), TargetRef::Global(id) if id.0 == malformed),
                "{malformed} 应落回全局 id"
            );
        }

        assert_eq!(
            TargetRef::parse("a:b:codex"),
            TargetRef::Project {
                project_id: "a".to_string(),
                agent_id: AgentTargetId("b:codex".to_string()),
            }
        );
    }
}
