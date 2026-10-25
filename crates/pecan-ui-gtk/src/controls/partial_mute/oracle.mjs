// Optional source-oracle tooling only. Cargo consumes the committed JSON.
// No npm packages, display, backend, profiles or device data are accessed.
import {readFileSync,writeFileSync} from "node:fs";
import {createHash} from "node:crypto";
const root=new URL("../../../../../",import.meta.url);
const mixer=readFileSync(new URL("src/mixer.ts",root),"utf8");
const ui=readFileSync(new URL("src/Mixer.tsx",root),"utf8");
const history=readFileSync(new URL("src/handelingen.ts",root),"utf8");
const card=ui.slice(ui.indexOf("function Kaart("));
function body(source,marker){
  const start=source.indexOf(marker);if(start<0)throw new Error(marker);
  const open=source.indexOf("{",start+marker.length);let i=open+1,depth=1;
  for(;depth&&i<source.length;i++){if(source[i]==="{")depth++;if(source[i]==="}")depth--;}
  if(depth)throw new Error(`Unbalanced ${marker}`);return source.slice(open+1,i-1);
}
const off=Number(mixer.match(/export const UIT_DB = (-?\d+)/)[1]);
const stripDb=new Function("s","UIT_DB",body(mixer,"export function stripDb("));
const metDb=new Function("s","db","UIT_DB",body(mixer,"export function metDb("));
const muted=new Function("bus","heel","s","voorMute","db","UIT_DB",`return ${card.match(/const gemute = ([\s\S]*?);/)[1]}`);
const toggle=new Function("bus","heel","s","muteStap","voorMute","gemute","db","metDb","UIT_DB","zetVolume","volumeVastleggen",
  body(card,"const mute = () =>").replace(": number[] | null",""));
const gain=new Function("nieuw","bus","s","metDb","zetVolume","heel","voorMute",body(card,"const zetDb = (nieuw: number) =>"));
const record=new Function("knoop","oud","nieuw","vastleggen","zetVolume",body(history,"export function volumeVastleggen("));
const key="synthetic|destinations|pair B";
const toggleAt=(channels=[2,3],k=key)=>({op:"toggle",channels,key:k});
const fader=(db,channels=[2,3],k=key)=>({op:"fader",db,channels,key:k});
const observe=volume=>({op:"observe",volume});
function run(scenario){
  let volume=scenario.initial.map(Math.fround);const memory=new Map(),expected=[];
  for(const step of scenario.steps){
    if(step.op==="observe"){volume=step.volume.map(Math.fround);continue;}
    const s={sleutel:step.key,kanalen:step.channels,knoop:{id:1,volume:[...volume],mute:false}};
    const before=[...volume],db=stripDb(s,off),heel=s.kanalen.length===volume.length;
    const gemute=muted(null,heel,s,memory,db,off);let after=null,undo=null,capture=null;
    const write=(_id,v)=>{if(capture)capture.push([...v]);else after=[...v];};
    const vastleggen=(_label,redo,back)=>{
      capture=[];back();redo();undo={before:capture[0],after:capture[1]};capture=null;
    };
    const remember=(id,old,next)=>record(id,old,next,vastleggen,write);
    if(step.op==="toggle")toggle(null,heel,s,()=>{throw new Error("Whole-node mute in partial oracle");},memory,gemute,db,(s,d)=>metDb(s,d,off),off,write,remember);
    else gain(step.db,null,s,(s,d)=>metDb(s,d,off),write,heel,memory);
    if(!after)throw new Error("No partial gain proposal");
    volume=after.map(Math.fround);
    const observed={...s,knoop:{...s.knoop,volume}};
    expected.push({kind:step.op==="fader"?"fader":gemute?"restore":"mute",before,after,
      undo,muted:muted(null,heel,observed,memory,stripDb(observed,off),off),
      memory:[...memory].sort(([a],[b])=>a<b?-1:a>b?1:0)});
  }
  return {...scenario,expected};
}
const scenarios=[
  {name:"unequal_pair_restores_mean_and_undo_retains_full_vector",initial:[.2,.9,.25,.75],steps:[toggleAt(),toggleAt()]},
  {name:"zero_pair_toggles_memory_without_a_history_change",initial:[1,.5,0,0],steps:[toggleAt(),toggleAt(),toggleAt()]},
  {name:"repeated_toggle",initial:[.1,.2,.3,.8],steps:Array.from({length:8},()=>toggleAt())},
  {name:"partial_fader_clears_gain_memory",initial:[1,.5,.25,.75],steps:[toggleAt(),fader(-12),toggleAt(),toggleAt()]},
  {name:"moving_fader_to_off_clears_the_mute_marker",initial:[1,.5,.25,.75],steps:[toggleAt(),fader(-90),toggleAt(),toggleAt()]},
  {name:"external_positive_gain_is_remembered_on_next_mute",initial:[1,.5,.25,.75],steps:[toggleAt(),observe([1,.5,.1,.3]),toggleAt(),toggleAt()]},
  {name:"unselected_external_changes_survive_restore",initial:[1,.5,.25,.75],steps:[toggleAt(),observe([.8,.7,0,0]),toggleAt()]},
  {name:"below_off_mean_cannot_restore_subfloor_gain",initial:[1,.5,0,1e-7],steps:[toggleAt(),toggleAt()]},
  {name:"positive_gain",initial:[.2,.4,1,1.5],steps:[toggleAt(),toggleAt()]},
  {name:"mono_selection",initial:[.2,.4,.6,.8],steps:[toggleAt([3]),toggleAt([3])]},
  {name:"noncontiguous_selection",initial:[.2,.4,.6,.8],steps:[toggleAt([0,3]),toggleAt([0,3])]},
  {name:"exact_keys_keep_sibling_memory",initial:[.2,.4,.6,.8],steps:[toggleAt([0,1],"synthetic|destinations|pair A"),toggleAt(),fader(0),toggleAt([0,1],"synthetic|destinations|pair A")]},
  {name:"case_and_prefix_are_distinct_keys",initial:[.2,.4,.6,.8],steps:[toggleAt([0,1],"synthetic|row|pair"),toggleAt([2,3],"synthetic|row|pair extra"),fader(0,[0,1],"Synthetic|row|pair"),toggleAt([2,3],"synthetic|row|pair extra")]},
];
const fixture={sources:["src/Mixer.tsx::Kaart","src/mixer.ts::stripDb/metDb","src/handelingen.ts::volumeVastleggen"],
  sha256:[ui,mixer,history].map(s=>createHash("sha256").update(s).digest("hex")),scenarios:scenarios.map(run)};
writeFileSync(new URL("oracle.json",import.meta.url),[
  "{",`  "sources":${JSON.stringify(fixture.sources)},`,`  "sha256":${JSON.stringify(fixture.sha256)},`,
  '  "scenarios":[',fixture.scenarios.map(s=>"    "+JSON.stringify(s)).join(",\n"),"  ]","}",""].join("\n"));
console.log(`${scenarios.length} synthetic partial-mute source traces generated`);
