# JohnnyGuitar NVSE, batch 6 (2026-11-08, #1007)

Actor values by code, actors' states and temporary effects, perks, karma, cells (visited,
expired, lighting templates), the moon, radios and talking activators, references'
activation prompts and primitives, effect shaders, mines, and the per-reference calls, in
`crates/fnv-game/src/nvse_jg6.rs`. Sources: the GECK wiki export [documented: the page
named]; the record layouts from `schemas/fnv` (xEdit wbDefinitionsFNV.pas); the base
game's records read with `fnvtool esm inventory` and `decode` [observed]. Probe set `jg6`
(`tools/capture/jip_probes.py`) and lab scenario `L5.ext.jg6` are written but **not run in
the game yet**: every [hypothesis] below waits for them. Tests: `crates/fnv-game/tests/jg6.rs`.

| Function | NVForever |
|---|---|
| DamageActorValueAlt, ForceActorValueAlt, ModActorValueAlt, RestoreActorValueAlt, SetActorValueAlt `code value` (and their …AVAlt aliases) | the game's own DamageAV, ForceAV, ModAV, RestoreAV and SetAV with a numeric actor value code [documented: each page's summary and example, e.g. ModActorValueAlt 5 3 raising Strength by 3]; JohnnyGuitar's actor value change handlers see the change as for the plain function. A code past the actor value table changes nothing [hypothesis] |
| UnforceAV `av` | nothing [documented: "currently broken and does nothing"] |
| UnsetAV `av` | the base goes back to what it was before the first SetAV on that actor value; the modifiers and the damage stay; nothing without a SetAV [documented: UnsetAV; hypothesis: "its previous state" read as the base SetAV changed, the first one kept] |
| GetActorValueModifierAlt (GetAVModAlt) `av [filter]` | 1 the permanent modifier, 2 the temporary one, 3 or none both [documented]; another filter 0 [hypothesis] |
| RewardKarmaAlt `value` | RewardKarma's change without the corner message: the karma note for the engine is dropped [documented] |
| GetLifeState | 0 alive, 2 dead, 3 unconscious (SetUnconscious or knocked down), 5 restrained, 6 an essential actor knocked down at no health [documented: the codes; hypothesis: which knock-downs read 6]. 1 (dying) is never read: NVForever's actors die at once [hypothesis: no dying timer] |
| IsInDialogueWithPlayer | 1 for the speaker of the player's conversation [documented] |
| GetCurrentFurnitureRef | the furniture reference the actor uses, 0 for none [documented; hypothesis: the 0] |
| HasHealthDamageEffect | an active effect on the actor whose value change lowers Health [documented; hypothesis: what counts as health-damaging] |
| GetTempIngestibleEffects | the player's ingestibles (ALCH) with effects running, each `[item, seconds left, duration]` of its effect with the longest duration [documented], in the order of their first effect [hypothesis] |
| ModNthTempEffectTimeLeft `index modifier` | GetTempEffects' Nth entry (`nvse_jip3`): its elapsed time goes down by `modifier`; 1 when there is such an entry, else 0 [documented: "modifies time elapsed"] |
| GetAvailablePerks | the perks the level-up menu offers now: the perks page's bright rows, in its order (`levelup::PerkSheet`) [documented] |
| GetHotkeySlot | the hotkey (1-8) the inventory reference's item is on, 0 for none [documented], over the hotkey table of SetHotkeyItem (`nvse_other`) |
| SendTrespassAlarmAlt | the trespass alarm for the owner of the calling reference (any reference): each witness who stands for that owner counts the trespass and turns on the player (`Game::trespass_alarm_for`) [documented: "uses the calling reference's owner"; hypothesis: the witnesses are the ones told]. Nothing for a reference no one else owns |
| IsCellVisited `cell` | 1 when the player's loaded area has ever held the cell [documented; hypothesis: every cell loaded around the player counts]. Kept in `FormChanges::visited_cells` (saved) |
| IsCellExpired `cell` | -1 not visited or the player's own cell, 1 after ResetInterior or when left for iHoursToRespawnCell game hours (not in a zone that never resets), else 0 [documented; the timing: `crate::respawn`] |
| GetLightingTemplateCell, SetLightingTemplateCell `template [cell]` | the cell the setter gave the template, 0 before; no cell clears it [documented; hypothesis: a lighting template record holds DATA and DALC only (schema), so the link is the session's]. A form that is not an LGTM or a cell changes nothing |
| GetMoonPhase | 0 full … 4 new … 7 waxing gibbous [documented]: the days passed over the climate's phase length (TNAM, low six bits) as the sky draws it (`fnv_world::sky::moon_phase`); the climate of the player's worldspace, phase 0 indoors [hypothesis] |
| GameGetSecondsPassed (GGetSecPass) | the frame step [documented: "the time elapsed since the game's last frame"] |
| GetBufferedCellsAlt `interiors` | the loaded cells, the interiors or the exteriors, by FormID [documented; hypothesis: NVForever keeps no cell buffer beyond the loaded area] |
| GetAllGameRadios | the placed references of every talking activator with the Radio Station header flag (0x00020000, radio.md §2), by FormID [documented] |
| GetTalkingActivatorActor `activator` | the actor SetTalkingActivatorActor gave the reference (`crate::talking_activators`); a base form reads its first placed reference with one, by FormID [hypothesis]; 0 for none |
| GetRefActivationPromptOverride, SetRefActivationPromptOverride `[prompt]` | the reference's XATO, "" for none; the setter keeps the new prompt for the session (`nvse_jg6::activation_prompt`) [documented] |
| GetPrimitiveType | 1 box, 2 sphere [documented]; 0 for none or one RemovePrimitive took away [hypothesis]. The reference table keeps box and sphere primitives only, so type 3 reads 0 |
| RemovePrimitive | 1 when the reference had a primitive [documented]; GetPrimitiveType reads 0 after |
| GetPlayingEffectShaders | the effect shaders scripts play on the reference, in the order they started [documented] |
| GetMineArmedEx | a live mine not going off, the player's included [documented: "works as expected on player-placed mines"]; the engine's mines fill the set (`nvf-play` mines.rs) |
| CallPerRef, CallPerRefEx `function [type(s)] [distance] [angle] [cell]` | the function runs once per reference of the loaded cells (or of `cell`, its persistent references when it is not loaded), the reference its argument; not the calling reference or disabled ones; a distance or an angle (from the calling reference, else the player) leaves out those farther or outside the heading [documented]. Order: the reference walk's (GetFirstRef) [hypothesis]. An error in the function stops the calls and the calling script [hypothesis] |
| CallPerMobileObject, CallPerMobileObjectEx `function level [type(s)]` | once per actor of the processing level: 0 the actors of the loaded cells, 3 the others, 1 and 2 none, as GetActorsByProcessingLevel (`nvse_gaps`), by FormID; not the calling reference or disabled ones [documented: the levels' meanings; hypothesis: NVForever runs every loaded actor fully] |

Follow-ups (engine): the crosshair reads a reference's prompt from the plugins, not
`nvse_jg6::activation_prompt`; triggers and collision keep a removed primitive; the
lighting template's cell link changes no lighting (the engine reads the template's own
DATA); NVForever has no dying state.

Not in this batch, still on the census (147 JohnnyGuitar names): GetRunSpeed (the run
speed is the engine's animation measure) and GetLandTextureUnderFeet (the game holds no
landscape layers at run time), with the UI, sound, camera, NiNode, texture, navmesh and
event-handler families for the batches after.
