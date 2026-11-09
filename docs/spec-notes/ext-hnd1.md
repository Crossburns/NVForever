# Extender functions High and Dry needs: batch hnd1 (2026-11-09)

The functions High and Dry's scripts call most that were not built, ranked by a headless
new-game run of the modpack (`fnvtool script run`, 300 frames). Evidence: the parameter
and return types the original game lists (DumpDocs, `schemas/script/opcode_names.json`),
and probe set `quests2` run in the original with High and Dry's plugins from a save in
Goodsprings [observed]. Built in `crates/fnv-game/src/nvse_hnd1.rs`.

| Function | What the original answered | NVForever |
|---|---|---|
| GetQuests flag | 0: VMQ01 (running) and VCG02 (neither running nor completed); 1, 2, 3, 4, 8: VCG01 (completed) | 0: quests not completed with an objective shown; other flags: completed quests [hypothesis beyond the save's three quests] |
| GetQuestObjectives quest flag | VMQ01, flag 0: 10 30 25; flags 1-4: no array | flag 0: shown objectives, ascending (the original's order is open); other flags: 0 |
| GetObjectiveTargets quest index | VMQ01 objective 10: no array | the objective's QSTA targets, 0 when none |
| GetActorLeveledList | a placed actor (GSBigHorner): nothing | the placed record's base when it is a leveled actor list, else 0 |
| GetSequenceAnimGroup sequence | slot 0: 0 (Idle) on the player and a standing actor; slots 1-4: -1 | slot 0: the group playing (PlayGroup's, else the body's movement group); others -1 [hypothesis: weapon and idle slots] |

JIP's list functions give 0, not an empty array, when there is nothing to list
[observed: GetObjectiveTargets, GetQuestObjectives flags 1-4].
