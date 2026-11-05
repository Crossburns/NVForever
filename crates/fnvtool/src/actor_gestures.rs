//! `fnvtool actor gestures`: the talking and listening gestures the IDLE tree gives
//! actors in the dialogue menu (docs/spec-notes/idles.md §12). For each actor, in a game
//! where nothing has happened yet, it is put in a conversation with the player saying
//! `--line` (its mood is that line's first response's; without it Neutral) and the
//! tree is asked many times while it talks, then as many times while it listens (the
//! player choosing) (`Game::talk_gesture`, without replay
//! delays). Each pick is reported to the rules as played, as the engine does, so the
//! tree's `IsLastIdlePlayed` conditions see it. The picks are counted per idle and per
//! group (the node above the idle) (fnv.actor.gestures/1).

use fnv_esm::{LoadOrder, Schema};
use fnv_game::dialogue::{Conversation, Line};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The people's animation folder below `meshes\` (the engine's `idles::PEOPLE_FOLDER`).
const PEOPLE_FOLDER: &str = "characters\\_male";

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, actors: &[String], line: Option<&str>, checks: u32) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    let mut rows = Vec::new();
    for name in actors {
        let mut g = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
        let actor = crate::actor_factions::resolve(&g, refs, name)?;
        // The conversation as the dialogue screen holds it while the actor says `line`
        // (no line: a line of no topic, whose mood is Neutral).
        let info = match line {
            Some(l) => data.by_edid(l).or_else(|| u32::from_str_radix(l.trim_start_matches("0x"), 16).ok().map(FormId)).ok_or_else(|| anyhow::anyhow!("no line {l}"))?,
            None => FormId(0),
        };
        let topic = data.infos.get(&info).and_then(|d| d.topic).unwrap_or(FormId(0));
        g.dialogue = Some(Conversation { actor, through: None, line: Some(Line { topic, info, response: 0, remaining: 10.0 }), choices: Vec::new(), topic_list: false, lines: 0, then: None, ending: false });
        let emotion = g.dialogue_emotion(actor);
        let mut parts = serde_json::Map::new();
        for talking in [true, false] {
            if !talking {
                // The player chooses: the actor says nothing.
                if let Some(c) = g.dialogue.as_mut() {
                    c.line = None;
                }
            }
            let mut by_idle: BTreeMap<String, u32> = BTreeMap::new();
            let mut by_group: BTreeMap<String, u32> = BTreeMap::new();
            let mut none = 0;
            for _ in 0..checks {
                match g.talk_gesture(actor, FormId::PLAYER_REF, PEOPLE_FOLDER, &|_| false) {
                    Some((idle, way)) => {
                        *by_idle.entry(data.name(idle)).or_default() += 1;
                        let group = way.len().checked_sub(2).map_or(String::new(), |i| data.name(way[i]));
                        *by_group.entry(group).or_default() += 1;
                        g.set_idle_playing(actor, Some(idle));
                        g.set_idle_playing(actor, None);
                    }
                    None => none += 1,
                }
            }
            parts.insert(
                if talking { "talking" } else { "listening" }.into(),
                json!({ "gestures": checks - none, "none": none, "by_group": by_group, "by_idle": by_idle }),
            );
        }
        rows.push(json!({
            "actor": data.name(actor),
            "line": line,
            "emotion": emotion,
            "checks": checks,
            "talking": parts["talking"],
            "listening": parts["listening"],
        }));
    }
    Ok(json!({ "schema": "fnv.actor.gestures/1", "actors": rows }))
}
