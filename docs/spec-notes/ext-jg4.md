# JohnnyGuitar NVSE, batch 4 (2026-11-01, #1007)

Idle markers' animations and traits, regions' weather lists, race flags and faction
members, in `crates/fnv-game/src/nvse_jg4.rs`. Sources: the GECK wiki export
[documented: the page named]; the record layouts from `schemas/fnv` (xEdit
wbDefinitionsFNV.pas). Not probed in the game (the JohnnyGuitar probe sets run on the
other machine); every value comes from a record. Setters change the record for the
session, read by the getters. Tests: `crates/fnv-game/tests/jg4.rs`.

| Function | NVForever |
|---|---|
| GetIdleMarkerAnimations `marker` | the marker's idles (IDLA) as an array, in record order [documented] |
| GetIdleMarkerTraitNumeric `marker trait` | 1 the flags (1 run in sequence, 4 do once; IDLF), 2 the idle timer (IDLT), 3 the number of animations; 0 for another trait [documented: the page's table] |
| SetIdleMarkerTraitNumeric `marker trait value` | 1 the flags, 2 the timer; the count is not set here [documented: the page and its note]; a timer that is not finite as a float, or below 0, is refused [hypothesis] |
| SetIdleMarkerAnimation `marker index idle` | the idle at the index replaced [documented]; an index past the list or a form that is not an IDLE changes nothing [hypothesis] |
| SetIdleMarkerAnimations `marker array` | the idles become the array's, in its order, and the count follows [documented: the note]; entries that are not IDLE forms are left out [hypothesis] |
| GetRegionWeathers `region` | the weathers of the region's weather block (RDWT after an RDAT of type 3) as an array, in record order [documented; the block: xEdit wbRegionDataEntries] |
| GetNthRegionWeatherType, …Chance, …Global `region index` | the entry's weather, chance, global; none (0) past the list [documented] |
| IsWeatherInRegion `region weather` | 1 when the block lists the weather [documented] |
| AddRegionWeather `region weather chance [global]` | the weather added with its chance (0-100) and global [documented]; a weather already listed is not listed again, and a region without a weather block gains one [hypothesis] |
| RemoveRegionWeather `region weather`, ClearRegionWeathers `region` | the weather's entry removed; every entry removed [documented] |
| GetRegionWeatherOverride, SetRegionWeatherOverride `region [flag]` | the weather block's RDAT flags bit 0 [documented: "an override flag"; the bit: `fnv_world::sky::REGION_WEATHER_OVERRIDE`, hypothesis there] |
| GetRegionWeatherPriority, SetRegionWeatherPriority `region [priority]` | the weather block's RDAT priority, a byte, so the setter keeps 0-255 [documented; hypothesis: the clamp] |
| SetRaceFlag `race flag on` | 0 playable, 2 child: the RACE DATA flag set or cleared, read by GetRaceFlag (`nvse_plugins`) [documented: the page's table]; another flag changes nothing [hypothesis] |
| GetFactionMembers `faction [rank]` | the base actors whose own faction list (SNAM) holds the faction, at that rank when one is given, in FormID order [documented; hypothesis: the record's list, not a template's, and the order] |

The engine's idles play a marker's changed collection (`nvse_jg4::idle_marker`, read by
`engine/nvforever/src/idles.rs`).

Follow-ups: the sky (`engine/nvforever/src/sky.rs`) loads the regions' weather lists once
from the plugins when it is set up, so a script's region weather changes reach the
getters only, not the weather drawn; it needs a reload when `OtherState::region_weathers`
changes; the light a region's weathers give (`fnv-world/src/lighting.rs`, `emittance`)
reads RDWT from the plugins too. The sandbox (`fnv-ai` sandbox.rs) still picks idle markers
and the stay at one from the record, so a marker a script emptied still draws actors, who
then fall back to their package's idles. The race flags reach GetRaceFlag only.

Not in this batch: GetLightingTemplateCell and SetLightingTemplateCell (the page names a
cell the template copies its values from; the LGTM record has only DATA in the schema, so
where that cell is kept is not known), GetRegionMapName and SetRegionMapName (RDMP is not decoded yet).
