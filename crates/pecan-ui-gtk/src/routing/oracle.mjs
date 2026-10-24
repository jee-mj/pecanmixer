// Optional source-oracle generation, Node >=22.13 built-in type stripping only.
// Reads public source and synthetic fixtures; imports no app/native API, accesses
// no profiles, and records commands in memory without executing them.
import {readFileSync,writeFileSync} from 'node:fs';
import {stripTypeScriptTypes} from 'node:module';
import {createHash} from 'node:crypto';
const root=new URL('../../../../',import.meta.url);
const source=name=>readFileSync(new URL(`src/${name}`,root),'utf8');
const inputs=Object.fromEntries(['api.ts','indeling.ts','bussen.ts','mixer.ts','App.tsx'].map(n=>[n,source(n)]));
const compile=text=>stripTypeScriptTypes(text).replace(/^import[\s\S]*?from\s*["'][^"']+["'];/gm,'').replace(/\bexport\s+/g,'');
const BUS=inputs['api.ts'].match(/export const BUS = ("[^"]+");/)[1];
const isBus=new Function('k',`return k.naam === ${BUS};`);
const helpers=new Function('isBus',compile(inputs['indeling.ts'])+';return {apparaatTitel,appNaam,poortLabel};')(isBus);
let commands=[];
const ontkoppel=ids=>{for(const id of ids)commands.push({kind:'disconnect',id});};
const verbind=(from,to)=>commands.push({kind:'connect',from,to});
const zetDoel=(node,target)=>commands.push({kind:'target',node,target});
const defs=new Function('isBus','ontkoppel','verbind','zetBus',compile(inputs['bussen.ts'])+';return bussen;')(isBus,ontkoppel,verbind,()=>{throw Error('No bus effect allowed');});
const mixer=new Function('isBus','ontkoppel','verbind','zetDoel',...Object.keys(helpers),compile(inputs['mixer.ts'])+';return {bouwMixer,zetBusDefs,schakel,telHandwerk,terugNaarAuto};')(isBus,ontkoppel,verbind,zetDoel,...Object.values(helpers));
const config=JSON.parse(readFileSync(new URL('fixtures/buses.json',import.meta.url)));
mixer.zetBusDefs(defs({bussen:config.widths,stand:config.states,midi:false}));
const base=JSON.parse(readFileSync(new URL('fixtures/graph.json',import.meta.url)));
const selection=inputs['App.tsx'].slice(inputs['App.tsx'].indexOf('  const uitgangen ='),inputs['App.tsx'].indexOf('  const kiesBewerkt ='));
const selected=new Function('mixer','graaf','bewerktSleutel','busSleutel',selection+';return {output:bewerkt?.sleutel??null,rec:bewerkteBus?.sleutel??null};');
const variants=[
 ['baseline',g=>g],
 ['detached_player_with_rec_routes',g=>{g.kabels=g.kabels.filter(l=>![1000,1001].includes(l.id));return g;}],
 ['target_override',g=>{g.knopen.find(n=>n.id===10).doel='21001';return g;}],
 ['missing_defaults',g=>{g.standaard_sink=null;g.standaard_bron=null;return g;}],
 ['missing_rec',g=>{g.knopen=g.knopen.filter(n=>n.id!==30);g.poorten=g.poorten.filter(p=>p.knoop!==30);g.kabels=g.kabels.filter(l=>l.in_knoop!==30);return g;}],
 ['disconnected',g=>{g.verbonden=false;g.knopen=[];g.poorten=[];g.kabels=[];return g;}],
];
const scenarios=[];
for(const [name,mutate] of variants){
 const graph=mutate(structuredClone(base));const m=mixer.bouwMixer(graph);
 const sources=[...m.apps,...m.hardware],routes=[];
 for(const s of sources)for(const d of m.bestemmingen){
  commands=[];const r=m.route(s,d);mixer.schakel(m,graph,s,d);
  routes.push({source:s.sleutel,destination:d.sleutel,state:r.stand.soort==='uit'?{kind:'off'}:r.stand.soort==='aan'?{kind:'on',manual:r.stand.handmatig}:{kind:'custom',explanation:r.stand.uitleg},commands});
 }
 const selections=[];
 for(const output of [null,'missing',...m.bestemmingen.filter(d=>!d.bus).map(d=>d.sleutel)])for(const rec of [null,'missing',...m.bestemmingen.filter(d=>d.bus).map(d=>d.sleutel)])selections.push({output,rec,expected:selected(m,graph,output,rec)});
 commands=[];mixer.terugNaarAuto(m);
 scenarios.push({name,graph,routes,selections,reset:{counts:{links:m.handwerk.kabels.length,targets:m.handwerk.doelen.length,detached:m.handwerk.losse},total:mixer.telHandwerk(m),commands}});
}
const hashes=Object.fromEntries(Object.entries(inputs).map(([n,s])=>[n,createHash('sha256').update(s).digest('hex')]));
writeFileSync(new URL('oracle.json',import.meta.url),JSON.stringify({synthetic:true,hashes,scenarios},null,2)+'\n');
console.log(`${scenarios.length} synthetic source-oracle scenarios; ${scenarios.reduce((n,s)=>n+s.routes.length,0)} route cases; ${scenarios.reduce((n,s)=>n+s.selections.length,0)} selection cases`);
