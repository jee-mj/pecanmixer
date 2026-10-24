// Test-only fixture generator. Execute unchanged callback bodies extracted from
// the retained JS oracle; no npm packages, GTK, audio server or user data.
// Cargo tests read the committed JSON and do not invoke Node.
import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const root = new URL("../../../../../", import.meta.url);
const source = readFileSync(new URL("src/Mixer.tsx", root), "utf8");
const mixer = readFileSync(new URL("src/mixer.ts", root), "utf8");
function body(marker) {
  const scope = marker.startsWith("on") || marker.startsWith("const zet")
    ? source.slice(source.indexOf("function Fader(")) : source;
  const start = scope.indexOf(marker);
  if (start < 0) throw new Error(`Missing source marker: ${marker}`);
  const open = scope.indexOf("{", start + marker.length);
  let depth = 1, i = open + 1;
  for (; depth && i < scope.length; i++) {
    if (scope[i] === "{") depth++;
    if (scope[i] === "}") depth--;
  }
  if (depth) throw new Error(`Unbalanced oracle: ${marker}`);
  return scope.slice(open + 1, i - 1);
}
const off = Number(mixer.match(/export const UIT_DB = (-?\d+)/)[1]);
const max = Number(mixer.match(/export const MAX_DB = (-?\d+)/)[1]);
const law = new Function("UIT_DB", "MAX_DB", `return ${source.match(/const WET:[\s\S]*?= (\[[\s\S]*?\n\]);/)[1]}`)(off, max);
const toDb = new Function("stand", "WET", "UIT_DB", "MAX_DB", body("function standNaarDb("));
const toPosition = new Function("db", "WET", "UIT_DB", body("function dbNaarStand("));
const position = db => toPosition(db, law, off);
const db = p => toDb(p, law, off, max);
const down = new Function("e", "baan", "stand", "sleept", "begin", "klaar", "zet", "standNaarDb", "UIT_DB",
  body("onPointerDown={(e) =>").replaceAll(": PointerEvent", ""));
const wheel = new Function("e", "db", "wiel", "begin", "onChange", "klaar", "window", "MAX_DB", "UIT_DB",
  body("onWheel={(e) =>"));
const reset = new Function("begin", "onChange", "klaar", body("onDoubleClick={() =>"));
const setBody = body("const zet = (s: number) =>");

function run({name, initial_db, steps}) {
  let observed = initial_db, now = 0, nextTimer = 1, context;
  const stand = {current: position(observed)}, sleept = {current: false};
  const wiel = {current: null}, baan = {current: {clientHeight: 160}};
  const listeners = new Map(), timers = new Map(), tokens = new Map(), expected = [];
  const emit = x => expected.push(x);
  const begin = () => {
    tokens.set(context.kind, context.token);
    emit({op: "begin", kind: context.kind, token: context.token});
  };
  const token = () => tokens.get(context.kind);
  const onChange = value => emit({op: "update", token: token(), db: value});
  const klaar = value => emit({op: "end", token: token(), db: value, reason: context.reason});
  const set = new Function("s", "stand", "onChange", "standNaarDb", "UIT_DB", setBody);
  const zet = p => set(p, stand, onChange, db, off);
  const window = {
    setTimeout(f, ms) { const id = nextTimer++; timers.set(id, {f, at: now + ms}); return id; }
  };
  // clearTimeout is global in the original handler. Supply this exact timer
  // cancellation scope, without rewriting its interaction body.
  const oldClear = globalThis.clearTimeout;
  globalThis.clearTimeout = id => timers.delete(id);
  const element = {
    setPointerCapture() {},
    addEventListener(name, fn) { listeners.set(name, fn); },
    removeEventListener(name) { listeners.delete(name); },
  };
  try {
    for (const step of steps) {
      if (step.at_ms !== undefined) now = step.at_ms;
      switch (step.op) {
        case "observe": observed = step.db; if (!sleept.current) stand.current = position(observed); break;
        case "down":
          context = {kind: "pointer", token: step.token};
          down({button: step.button ?? 0, clientY: step.y, pointerId: 1, currentTarget: element, preventDefault() {}},
            baan, stand, sleept, begin, klaar, zet, db, off);
          break;
        case "move":
          context = {kind: "pointer"}; baan.current = step.height === undefined ? null : {clientHeight: step.height};
          listeners.get("pointermove")?.({clientY: step.y, shiftKey: step.shift ?? false}); break;
        case "up": case "cancel":
          context = {kind: "pointer", reason: step.op === "up" ? "released" : "pointer_cancelled"};
          listeners.get(step.op === "up" ? "pointerup" : "pointercancel")?.(); break;
        case "wheel":
          context = {kind: "wheel", token: step.token};
          wheel({deltaY: step.delta_y, shiftKey: step.shift ?? false}, observed, wiel, begin, onChange, klaar, window, max, off);
          break;
        case "reset":
          context = {kind: "reset", token: step.token, reason: "reset"}; reset(begin, onChange, klaar); break;
        case "advance":
          context = {kind: "wheel", reason: "wheel_idle"};
          for (const [id, timer] of timers) if (timer.at <= now) { timers.delete(id); timer.f(); }
          break;
        default: throw new Error(`Unknown step ${step.op}`);
      }
    }
  } finally { globalThis.clearTimeout = oldClear; }
  return {name, initial_db, steps, expected, position: stand.current, observed_db: observed,
    dragging: sleept.current, wheel_deadline_ms: wiel.current === null ? null : timers.get(wiel.current)?.at};
}
const downAt = y => ({op:"down", token:1, y});
const move = (y, height=160, shift=false) => ({op:"move", y, height, shift});
const w = (at_ms, delta_y=-1, shift=false, token=1) => ({op:"wheel", at_ms, delta_y, shift, token});
const tick = at_ms => ({op:"advance", at_ms});
const observe = db => ({op:"observe", db});
const scenarios = [
  {name:"relative_drag_and_delayed_observation",initial_db:-10,steps:[downAt(100),move(80),observe(-9),move(60,160,true),{op:"up"}]},
  {name:"pointer_cancel_finalizes",initial_db:-10,steps:[downAt(100),move(60),{op:"cancel"}]},
  {name:"click_has_no_update",initial_db:-7.123,steps:[downAt(999),{op:"up"}]},
  {name:"secondary_ignored",initial_db:-10,steps:[{...downAt(10),button:2},move(0),{op:"up"}]},
  {name:"changing_height_and_shift",initial_db:0,steps:[downAt(100),move(110),move(130,300,true),{op:"move",y:90,shift:true},move(-10000),move(10000),{op:"up"}]},
  {name:"wheel_uses_observed_not_proposed",initial_db:-10,steps:[w(0),w(200,-1,false,2),tick(799),tick(800)]},
  {name:"wheel_uses_new_observation",initial_db:-10,steps:[w(0),observe(-9),w(200,-1,false,2),tick(800)]},
  {name:"wheel_new_group_at_deadline",initial_db:-10,steps:[w(0),tick(600),w(600,-1,false,2),tick(1200)]},
  {name:"queued_timer_replaced_before_delivery",initial_db:-10,steps:[w(0),w(800,-1,false,2),tick(800),tick(1400)]},
  {name:"wheel_reset_overlap_is_not_implicitly_cancelled",initial_db:-10,steps:[w(0),{op:"reset",token:2},observe(0),tick(600)]},
  {name:"wheel_and_drag_have_separate_refs",initial_db:-10,steps:[downAt(100),move(80),w(10,-1,false,2),observe(-9),move(60),{op:"up"},tick(610)]},
  {name:"reset_is_one_immediate_sequence",initial_db:-15,steps:[{op:"reset",token:7},observe(0)]},
];
for (const initial of [-100,-90,-60.05,-60,-59.95,-0.05,0,5.95,6,9]) {
  for (const shift of [false,true]) for (const delta of [-1,0,1]) {
    scenarios.push({name:`wheel_${initial}_${shift}_${delta}`,initial_db:initial,steps:[w(0,delta,shift),tick(600)]});
  }
}
for (const p of [0,.0049,.005,.0051,.04,.2,.42,.62,.76,.86,1]) {
  scenarios.push({name:`drag_law_boundary_${p}`,initial_db:-90,steps:[downAt(0),move(-p*136),{op:"up"}]});
}
const fixture = {
  source: "src/Mixer.tsx::Fader; src/mixer.ts constants",
  sha256: createHash("sha256").update(source).digest("hex"),
  generator: "node crates/pecan-ui-gtk/src/controls/fader/oracle.mjs",
  scenarios: scenarios.map(run),
  rounding: [-.05,.05,-.15,.15,-2.05,2.05,-6.05,6.05,-.049999999999999996,.049999999999999996].map(input => {
    const result = Math.round(input*10)/10;
    const bits = new DataView(new ArrayBuffer(8)); bits.setFloat64(0,input,false);
    return {input, input_bits:bits.getBigUint64(0,false).toString(16), expected:result, negative_zero:Object.is(result,-0)};
  }),
};
const output = new URL("oracle.json", import.meta.url);
const lines = ["{", `  "source": ${JSON.stringify(fixture.source)},`,
  `  "sha256": ${JSON.stringify(fixture.sha256)},`,
  `  "generator": ${JSON.stringify(fixture.generator)},`, "  \"scenarios\": [",
  fixture.scenarios.map(x => "    "+JSON.stringify(x)).join(",\n"),
  "  ],", "  \"rounding\": [", fixture.rounding.map(x => "    "+JSON.stringify(x)).join(",\n"), "  ]", "}", ""];
writeFileSync(output, lines.join("\n"));
console.log(`${fixture.scenarios.length} synthetic source-oracle traces -> ${fileURLToPath(output)}`);
