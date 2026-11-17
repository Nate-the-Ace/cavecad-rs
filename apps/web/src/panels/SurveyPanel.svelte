<script lang="ts">
  // The Survey panel: the in-cave side of CaveCAD. An instrument (a Cavway X1 over Bluetooth, or the
  // simulator) sends shots; each one is written to the Survey Notebook as it arrives, and the Map, Profile
  // and Cross Section beside this panel redraw from the notebook, so the cave grows while it is being
  // surveyed. Everything here is sized for a tablet held in a glove: big targets, one thing to do next.
  // The rules (what a reading is, where the surveyor stands next) are Rust: crates/cave-session, tested there.
  import FeatureShotPicker from '../components/FeatureShotPicker.svelte';
  import { symbolDef } from '../lib/symbols';
  import X1Marks from '../components/X1Marks.svelte';
  import { untrack } from 'svelte';
  import { Check, ChevronLeft, ChevronRight, Crosshair, Flashlight, FlashlightOff, Pause, Play, Radar, ScanLine, SkipForward, X } from '@lucide/svelte';
  import SweepDial from '../components/SweepDial.svelte';
  import QuickRef from '../components/QuickRef.svelte';
  import CalibrationGuide from '../components/CalibrationGuide.svelte';
  import { BookOpen, Star } from '@lucide/svelte';
  import SurveyOptions from '../components/SurveyOptions.svelte';
  import { Bluetooth, FlaskConical, Unplug } from '@lucide/svelte';
  import { phone } from '../lib/phone.svelte';
  import LengthField from '../components/LengthField.svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { SIDES, SIDE_LETTER, angleText, type Side } from '../lib/notebook';
  import { formatLength } from '../lib/units';
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';
  import { webBluetoothAvailable } from '../lib/instrument/webble';

  let ready = $state(false);
  let showCapture = $state(false);
  loadEngine().then(() => (ready = true));

  // Set up for the open trip (not on every edit: that would move the surveyor back to the page's end).
  $effect(() => {
    void cave.trip;
    void cave.opened;
    void ready;
    untrack(() => ready && live.syncToTrip());
  });

  const tripName = $derived.by(() => {
    void cave.version;
    return ready ? (engine.notebookTrips().find((t) => t.id === cave.trip)?.name || 'Untitled trip') : '';
  });

  // The oldest thing held for Accept, and what writing it would do, for the preview card.
  const held = $derived(survey.held[0] ?? null);

  // Passage dimensions at the station the surveyor stands at.
  let wallInput = $state<Record<Side, number | null>>({ left: null, right: null, up: null, down: null });
  let wallStation = $state<string | null>(null);
  let wallsFromEngine: Record<Side, number | null> = { left: null, right: null, up: null, down: null };
  $effect(() => {
    void cave.version;
    void survey.standingWalls;
    void ready;
    untrack(() => {
      const w = ready ? survey.standingWalls : null;
      wallStation = w?.station ?? null;
      wallsFromEngine = w ? { left: w.left, right: w.right, up: w.up, down: w.down } : { left: null, right: null, up: null, down: null };
      wallInput = { ...wallsFromEngine };
    });
  });
  // A dimension the person typed is written to the notebook; one the notebook refuses goes back to what it holds.
  $effect(() => {
    const typed = { ...wallInput };
    untrack(() => {
      if (wallStation === null) return;
      for (const s of SIDES) {
        if (typed[s] === wallsFromEngine[s]) continue;
        if (!live.setStandingWall(s, typed[s])) wallInput = { ...wallsFromEngine };
        break; // each write refreshes the panel, which settles the rest
      }
    });
  });

  const units = $derived(cave.units);
  const linked = $derived(survey.status === 'connected' || survey.status === 'lost' || survey.status === 'connecting');
  const noBluetooth = $derived(!webBluetoothAvailable());
  const dist = (m: number) => formatLength(m, units, 2);
  const sweep = $derived(survey.sweep);
  const lastSweep = $derived(survey.lastSweep);
  let quickRef = $state(false);
  const manual = $derived(survey.standard === 'manual');
  // The shot button says where the shot goes, so there is no doubt which station it lands on in the map.
  // LRUD and sweeps are taken at one station, facing on along the survey: only that station is named
  const atOnly = $derived(!survey.state.awaiting && (survey.state.mode === 'lrud' || !!survey.sweep || !!survey.waitingForDevice));
  // the X1's two shot buttons: one shot where you stand, and the leg to the next station
  const singleWhere = $derived.by(() => {
    const from = survey.state.from.trim() || '?';
    if (survey.sweep || survey.waitingForDevice) return `Sweep at ${survey.sweep?.station ?? from}`;
    return survey.state.mode === 'lrud' ? `LRUD at ${from}` : `Splay from ${from}`;
  });
  const legWhere = $derived.by(() => {
    const st = survey.state;
    const from = st.from.trim() || '?';
    const to = st.to.trim() || '?';
    return st.awaiting ? `Backsight ${to} → ${from}` : `${from} → ${to}`;
  });
  const shotWhere = $derived.by(() => {
    const st = survey.state;
    const from = st.from.trim();
    const to = st.to.trim();
    if (survey.sweep) return `Sweep at ${survey.sweep.station}`;
    if (survey.waitingForDevice) return `Sweep at ${from || '?'}`;
    if (st.awaiting) return `Backsight ${to || '?'} → ${from || '?'}`;
    if (!from) return 'Take shot';
    if (st.mode === 'splay') return `Splay from ${from}`;
    if (st.mode === 'lrud') return `LRUD at ${from}`;
    return to ? `Take shot ${from} → ${to}` : `Take shot from ${from}`;
  });
  const KIND_NAME = $derived(manual ? { lrud: 'LRUD', room: 'Room outline' } : { lrud: 'LRUD ring', room: 'Room sweep' });
  // Digital: every station gets a sweep; say so while the standing station has none yet
  const sweepDue = $derived(!manual && linked && !survey.sweep && survey.standing !== '' && survey.lastSweep?.station !== survey.standing && survey.log.length > 0);

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA') return; // typing: leave the keys to the field
    if (e.key === 'Enter' && survey.held.length > 0) live.accept();
    else if (e.key === 'Escape') live.discard();
    else if (e.key === ' ' && survey.canTakeShot) {
      e.preventDefault();
      live.pressShot();
    } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey ? engine.redo('notebook') : engine.undo('notebook')) changed();
    }
  }

  let featurePicker = $state(false);
  const symbolName = (key: string) => symbolDef(key)?.name ?? key.replace(/^sym-/, '').replace(/-/g, ' ');
  const enter = (e: KeyboardEvent) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur();
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: the panel takes Space, Enter, Escape and Ctrl/Cmd+Z for the survey -->
<div class="panel" role="region" aria-label="Survey" tabindex="-1" onkeydown={onKey}>
  <div class="bar">
    <PanelUndo scope="notebook" label="in the notebook" />
    <span class="chip" class:on={survey.status === 'connected'} class:warn={survey.status === 'lost' || survey.status === 'error'} role="status">
      {#if survey.status === 'connected'}{survey.device || 'Connected'}
      {:else if survey.status === 'connecting'}Connecting…
      {:else if survey.status === 'lost'}Reconnecting…
      {:else if survey.status === 'error'}Could not connect
      {:else if survey.status === 'unsupported'}No Bluetooth here
      {:else}No instrument{/if}
    </span>
    <span class="trip" title="The notebook page shots are written to (choose it in the Notebook)">{tripName}</span>
  </div>

  <div class="scroll">
    {#if survey.detail && survey.status !== 'connected'}
      <p class="banner" role="alert">{survey.detail}{#if survey.status === 'lost' || survey.status === 'connecting'} <button class="stoplink" onclick={live.disconnect}><Unplug size={16} /> Disconnect</button>{/if}</p>
    {:else if survey.status === 'lost' || survey.status === 'connecting'}
      <p class="banner" role="status">{survey.status === 'lost' ? 'Reconnecting…' : 'Connecting…'} <button class="stoplink" onclick={live.disconnect}><Unplug size={16} /> Disconnect</button></p>
    {:else if noBluetooth && !linked}
      <p class="banner">This browser has no Web Bluetooth. On an iPad, open CaveCAD in the <b>Bluefy</b> browser to connect an instrument. The simulator works anywhere.</p>
    {/if}

    {#if phone.on}
      <!-- the phone has no ribbon: connecting and the shot settings are here -->
      <section class="link" aria-label="Instrument">
        <div class="row">
          {#if linked}
            <button onclick={live.disconnect}><Unplug size={18} /> Disconnect</button>
          {:else}
            <button class="go" onclick={() => void live.connect('auto')}><Bluetooth size={18} /> Connect</button>
            <button onclick={() => void live.connect('simulator')}><FlaskConical size={18} /> Simulator</button>
          {/if}
        </div>
        <div class="opts"><SurveyOptions /></div>
      </section>
    {/if}

    <section class="device" aria-label="Device control">
      <div class="devhead">
        <h4>Device control</h4>
        
        <div class="seg" role="radiogroup" aria-label="Survey standard">
          <button role="radio" aria-checked={manual} class:on={manual} onclick={() => live.setStandard('manual')} title="Mostly sketched by hand: LRUD needs only the four walls, room outlines are rough">Manual</button>
          <button role="radio" aria-checked={!manual} class:on={!manual} onclick={() => live.setStandard('digital')} title="The Cavway captures the detail: dense rings at every station, full room outlines">Digital</button>
        </div>
      </div>
      <div class="tools">
        <button class="qr" onclick={() => (quickRef = true)} title="The Cavway X1's controls and calibration, as on the card in its case"><BookOpen size={16} /> X1 quick reference</button>
        {#if live.aimsFirst() && !survey.calibration}<button class="qr" onclick={live.startCalibration} title="Follow the X1's calibration shots and say where to point next">Guided calibration</button>{/if}
      </div>
      <p class="hint">{manual ? 'Manual: you sketch the detail. LRUD is four aimed shots, one to each wall: left, right, up, down. CaveCAD says which wall is next.' : 'Digital: the Cavway captures the detail. Sweep every station to 100% covered: a ring across the passage, rooms all the way round.'}</p>
      <label class="acc">Accuracy check
        <select value={survey.accuracy} onchange={(e) => live.setAccuracy(e.currentTarget.value as 'strict' | 'normal' | 'relaxed')} aria-label="How touchy to be about the instrument's accuracy complaints">
          <option value="strict">Strict: every X1 complaint</option>
          <option value="normal">Normal: 2% gravity, 5% magnetic, 5° dip, 2° angle</option>
          <option value="relaxed">Relaxed: twice Normal</option>
        </select>
      </label>
      <X1Marks />
      {#if sweepDue}<p class="due" role="status">Sweep {survey.standing} before moving on.</p>{/if}
      <div class="row">
        <button onclick={() => live.laser(true)} disabled={!survey.canLaser} title="Laser on, for aiming (the X1)"><Flashlight size={18} /> Laser on</button>
        <button onclick={() => live.laser(false)} disabled={!survey.canLaser} title="Laser off (the X1)"><FlashlightOff size={18} /> Laser off</button>
      </div>
      {#if survey.calibration}
        <CalibrationGuide guide={survey.calibration} />
      {:else if survey.waitingForDevice && !sweep}
        <div class="sweep waiting" role="status">
          <div class="sweepinfo">
            <p class="sweephead"><b>{KIND_NAME[survey.waitingForDevice]}</b> at {survey.standing}: ready</p>
            <p class="aim">Hold DIST+M on the X1 to start its scan.</p>
            <p class="hint">{survey.waitingForDevice === 'room' ? 'Keep the laser about level and turn right round.' : 'Sweep a slow ring across the passage.'} End the scan on the X1 (CLR) when the check here says complete: CaveCAD finishes the sweep by itself.</p>
          </div>
        </div>
        <div class="row"><button onclick={live.cancelDeviceSweep}><X size={18} /> Cancel</button></div>
      {:else if sweep}
        <div class="sweep" class:done={sweep.check.complete} role="status" aria-live="polite">
          <SweepDial check={sweep.check} />
          <div class="sweepinfo">
            <p class="sweephead"><b>{KIND_NAME[sweep.check.kind]}</b> at {sweep.station} · {manual ? `${sweep.check.shots} shot${sweep.check.shots === 1 ? '' : 's'}` : `${Math.floor(sweep.check.coverage)}% covered`}</p>
            {#if sweep.check.complete}
              <p class="ok"><Check size={16} /> Complete: everything it needs is there.</p>
            {:else if manual && sweep.check.aim}
              <p class="aim">Aim at {sweep.check.aim.wall} and shoot.</p>
            {:else}
              {#each sweep.check.todo as t}<p class="todo">{t}</p>{/each}
            {/if}
            {#each sweep.check.notes as t}<p class="hint">{t}</p>{/each}
            {#if sweep.check.lrud}
              {@const [l, r, u, d] = sweep.check.lrud}
              <p class="hint">From the ring: L {dist(l)} · R {dist(r)} · U {dist(u)} · D {dist(d)} (fills only the sides no tape measured)</p>
            {/if}
          </div>
        </div>
        <div class="row">
          {#if !sweep.armed}<p class="hint grow">Running on the X1: end the scan there (CLR), or finish here.</p>{/if}
          {#if sweep.armed && survey.canTakeShot && !manual}
            <button onclick={live.toggleFiring}>{#if survey.firing}<Pause size={18} /> Pause firing{:else}<Play size={18} /> Fire on its own{/if}</button>
          {/if}
          <button class="go" onclick={live.finishSweep}><Check size={18} /> Finish sweep</button>
        </div>
      {:else}
        <div class="row">
          <button class="sweepbtn" onclick={() => live.startSweep('lrud')} disabled={!linked} title="A slow ring across the passage at this station: the cross-section and its left, right, up and down"><ScanLine size={20} /> {manual ? 'LRUD' : 'LRUD ring'}<small>{manual ? 'one shot to each wall' : 'dense ring across the passage'}</small></button>
          <button class="sweepbtn" onclick={() => live.startSweep('room')} disabled={!linked} title="The laser about level, turned right round: the outline of a room"><Radar size={20} /> {manual ? 'Room outline' : 'Room sweep'}<small>{manual ? 'rough, eight ways round' : 'level, all the way round'}</small></button>
        </div>
        <p class="hint">Or hold DIST+M on the X1 for its own scan mode: CaveCAD checks it as
          <select value={survey.sweepKind} onchange={(e) => live.setSweepKind(e.currentTarget.value as 'lrud' | 'room')} aria-label="Check the X1's own sweeps as">
            <option value="lrud">an LRUD ring</option>
            <option value="room">a room sweep</option>
          </select>.
        </p>
        {#if lastSweep}
          <p class="hint" class:warnline={!lastSweep.check.complete}>Last sweep: {KIND_NAME[lastSweep.check.kind]} at {lastSweep.station}, {lastSweep.check.standard === 'digital' ? `${Math.floor(lastSweep.check.coverage)}% covered` : `${lastSweep.check.shots} shots`}, {lastSweep.check.complete ? 'complete' : `not complete (${lastSweep.check.todo[0] ?? ''})`}</p>
        {/if}
      {/if}
    </section>

    {#if survey.simNote}<p class="simnote" role="status">Simulating: {survey.simNote}</p>{/if}

    <section class="reading" aria-label="Latest reading">
      {#if survey.last}
        <div class="big"><b>{dist(survey.last.distance)}</b><small>tape</small></div>
        <div class="big"><b>{angleText(+survey.last.azimuth.toFixed(1))}°</b><small>compass</small></div>
        <div class="big"><b>{angleText(+survey.last.inclination.toFixed(1))}°</b><small>clino</small></div>
      {:else}
        <p class="idle">{linked ? 'Waiting for a shot…' : phone.on ? 'Connect an instrument above, or try the simulator.' : 'Connect an instrument in the Survey tab, or try the simulator.'}</p>
      {/if}
    </section>

    {#if survey.message || survey.warnings.length > 0}
      <div class="notes">
        {#if survey.message}<p class="problem" role="alert">{survey.message}</p>{/if}
        {#each survey.warnings as w}<p class="caution" role="status">{w}</p>{/each}
      </div>
    {/if}

    {#if survey.stored.length > 0}
      <section class="hold" aria-label="Shots from the instrument's memory">
        <p><b>{survey.stored.length} shot{survey.stored.length === 1 ? '' : 's'} from the instrument's memory</b>{survey.readingMemory ? ' (still arriving…)' : ''}: taken while it was not connected, maybe another day. They are not in the notebook.</p>
        <ul class="stored">
          {#each survey.stored.slice(0, 6) as r}<li>{dist(r.distance)} · {r.azimuth.toFixed(1)}° · {r.inclination.toFixed(1)}°{r.leg ? ' · leg' : ''}{r.scan ? ' · scan' : ''}</li>{/each}
          {#if survey.stored.length > 6}<li class="hint">and {survey.stored.length - 6} more</li>{/if}
        </ul>
        <div class="row">
          <button class="go" onclick={live.addStored}><Check size={20} /> Add at {survey.standing || survey.state.from}</button>
          <button class="no" onclick={live.discardStored}><X size={20} /> Discard</button>
        </div>
      </section>
    {/if}

    {#if held}
      <section class="hold" aria-label="Reading waiting">
        {#if !held.reject}
          <p>Write this as <b>{held.text}</b>?{#if survey.held.length > 1} <span class="hint">({survey.held.length - 1} more waiting)</span>{/if}</p>
          {#each held.warnings as w}<p class="caution">{w}</p>{/each}
        {:else}<p class="problem">{held.reject}</p>{/if}
        <div class="row">
          <button class="go" onclick={live.accept} disabled={held.reject !== null}><Check size={20} /> Accept</button>
          <button class="no" onclick={live.discard}><X size={20} /> Discard</button>
        </div>
      </section>
    {/if}

    <section class="where" aria-label="Where you are">
      <div class="names">
        <label>{atOnly ? 'At' : 'From'} <input class="name" value={survey.state.from} oninput={(e) => live.setNames(e.currentTarget.value, survey.state.to)} maxlength="40" aria-label="Station you are at" onkeydown={enter} disabled={survey.state.awaiting !== null} /></label>
        {#if !atOnly}
        <span aria-hidden="true">→</span>
        <label>To <input class="name" value={survey.state.to} oninput={(e) => live.setNames(survey.state.from, e.currentTarget.value)} maxlength="40" aria-label="Station the next shot goes to" onkeydown={enter} disabled={survey.state.mode === 'splay' || survey.state.awaiting !== null} /></label>
        {/if}
      </div>
      <p class="next" role="status">{survey.sweep || survey.waitingForDevice ? `Sweeping at ${survey.sweep?.station ?? survey.state.from}.` : survey.instruction}</p>
      {#if survey.groupsLegs && survey.waiting > 0}
        <p class="wait" role="status">{survey.waiting} close shot{survey.waiting === 1 ? '' : 's'} so far: {survey.waiting === 1 ? 'two more make' : 'one more makes'} a leg. A pause of a few seconds makes {survey.waiting === 1 ? 'it a splay' : 'them splays'}. <button class="link" onclick={live.settleNow}>Splays now</button></p>
      {:else}
        <p class="wait" aria-hidden="true">&nbsp;</p>
      {/if}
    </section>

    <section class="walls" aria-label="Passage dimensions">
      <h4>Passage at {wallStation ?? 'this station'}</h4>
      {#if wallStation}
        <div class="lrud">
          {#each SIDES as s}
            <span class="lr"><b>{SIDE_LETTER[s]}</b><LengthField id="survey-{s}" bind:value={wallInput[s]} min={0} /></span>
          {/each}
        </div>
      {:else}
        <p class="idle">Appears once this station has a line in the notebook (take its shot first).</p>
      {/if}
    </section>

    <section class="log" aria-label="Shots this session">
      <h4>This session</h4>
      {#each [...survey.log].reverse() as e (e.n)}
        <div class="entry" class:flag={e.warnings.length > 0} title={e.warnings.join(' ')}>
          <b>{e.label}</b>
          <span>{dist(e.reading.distance)} · {angleText(+e.reading.azimuth.toFixed(1))}° · {angleText(+e.reading.inclination.toFixed(1))}°</span>
        </div>
      {:else}
        <p class="idle">Nothing yet.</p>
      {/each}
    </section>

    <details class="capture" bind:open={showCapture}>
      <summary>Instrument log, and capture for an instrument that is not supported yet</summary>
      <p class="hint">Capture connects to any Bluetooth device and logs every frame it sends. The browser can only reach services it is told about in advance: list the service UUIDs (from a Bluetooth scanner app such as nRF Connect), separated by spaces or commas.</p>
      <label class="chk"><input type="checkbox" bind:checked={survey.anyDevice} /> Show every Bluetooth device when connecting (if the instrument is not offered by default)</label>
      <textarea bind:value={survey.services} rows="2" placeholder="service uuids, e.g. 0000ffe0-0000-1000-8000-00805f9b34fb" aria-label="Service UUIDs to reach"></textarea>
      <div class="row">
        <button class="small" onclick={() => live.connect('capture')} disabled={linked}>Connect in capture mode</button>
        <button class="small" onclick={() => navigator.clipboard?.writeText(survey.frames.join('\n'))} disabled={survey.frames.length === 0}>Copy log</button>
        <button class="small" onclick={() => (survey.frames = [])} disabled={survey.frames.length === 0}>Clear</button>
      </div>
      <pre aria-label="Frames received">{survey.frames.join('\n') || 'No frames yet.'}</pre>
    </details>
  </div>

  <footer class="dock" aria-label="Actions">
    {#if live.aimsFirst()}
      <!-- an X1: press once to turn the laser on and aim, press again to shoot -->
      <div class="shots">
        <button class="shoot" class:armed={survey.aiming === 'single'} onclick={() => live.pressShot('single')} disabled={!survey.canTakeShot || survey.burst > 0 || !!survey.state.awaiting}>
          <Crosshair size={20} />
          <span class="lines">
            <span class="where">{singleWhere}</span>
            <small>{survey.aiming === 'single' ? 'Laser on: press to shoot' : 'Single shot'}</small>
          </span>
        </button>
        <button class="shoot" class:armed={survey.aiming === 'leg'} onclick={() => live.pressShot('leg')} disabled={!survey.canTakeShot || survey.burst > 0 || (!survey.state.awaiting && (atOnly || !survey.state.to.trim()))}>
          <Crosshair size={20} />
          <span class="lines">
            <span class="where">{legWhere}</span>
            <small>{survey.burst > 0 ? `Shooting the leg: ${3 - survey.burst + 1} of 3…` : survey.aiming === 'leg' ? 'Laser on: press for 3 shots' : 'Station to station'}</small>
          </span>
        </button>
      </div>
    {:else}
    <button class="shoot"  onclick={() => live.pressShot()} disabled={!survey.canTakeShot}>
      <Crosshair size={22} />
      <span class="lines">
        <span class="where">{survey.kind === 'simulator' && !survey.sweep && !live.aimsFirst() ? shotWhere.replace('Take shot', 'Shoot station') : shotWhere}</span>
        {#if survey.aiming}<small>Laser on: aim, then press again to shoot</small>{:else if live.aimsFirst() && !survey.firing}<small>Press to aim the laser</small>{/if}
      </span>
    </button>
    {/if}
    {#if survey.feature}
      <div class="feat armed">
        <button class="shoot" onclick={live.shootFeature} disabled={!survey.canTakeShot}>
          <Star size={20} />
          <span class="lines">
            <span class="where">Feature: {survey.feature.label || symbolName(survey.feature.symbol)}</span>
            <small>Aim at it, then press (or DIST on the X1)</small>
          </span>
        </button>
        <button class="cancel" onclick={live.cancelFeature} title="The next shot is an ordinary one"><X size={18} /> Cancel</button>
      </div>
    {:else}
      <button class="featbtn" onclick={() => (featurePicker = true)} disabled={!survey.canTakeShot} title="Place a symbol where the laser hits, with a label: nothing is written to the notebook"><Star size={18} /> Feature shot</button>
    {/if}
    <div class="slots">
      <button onclick={live.previousStation} disabled={survey.state.awaiting !== null} title="Step back to the station before this one (undo is in the bar above)"><ChevronLeft size={18} /> Previous station</button>
      {#if survey.state.awaiting}
        <button onclick={live.skipBacksight}><SkipForward size={18} /> No backsight</button>
      {:else}
        <button onclick={live.nextStation} disabled={survey.state.mode === 'splay'} title="Move on to the next station without a shot">Next station <ChevronRight size={18} /></button>
      {/if}
    </div>
  </footer>
</div>
{#if quickRef}<QuickRef page="cavway-x1" onclose={() => (quickRef = false)} />{/if}
{#if featurePicker}<FeatureShotPicker onclose={() => (featurePicker = false)} onpick={(sym, label) => { featurePicker = false; live.armFeature(sym, label); }} />{/if}

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); font-size: 14px; outline: none; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .scroll { flex: 1; overflow-y: auto; overflow-x: hidden; padding: 10px; display: flex; flex-direction: column; gap: 12px; }
  .chip { padding: 3px 10px; border-radius: 999px; border: 1px solid var(--line); color: var(--muted); font-size: 12px; }
  .chip.on { border-color: var(--accent); color: var(--text); }
  .chip.warn { border-color: var(--warn); color: var(--warn); }
  .trip { color: var(--muted); font-size: 12px; max-width: 14em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { font: inherit; padding: 10px 14px; min-height: 44px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; display: inline-flex; align-items: center; justify-content: center; gap: 6px; touch-action: manipulation; }
  button:disabled { opacity: 0.45; cursor: default; }
  button.small { min-height: 34px; padding: 4px 10px; font-size: 13px; }
  button.go, button.shoot { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); }
  button.no { color: var(--danger); }
  button.shoot { font-size: 17px; font-weight: 600; min-height: 56px; width: 100%; }
  .featbtn { display: flex; align-items: center; justify-content: center; gap: 6px; min-height: 40px; font: inherit; color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: var(--radius); }
  .feat { display: flex; gap: 8px; }
  .feat .shoot { flex: 1; }
  .feat.armed .shoot { border-color: var(--warn); }
  .feat .cancel { display: flex; align-items: center; gap: 4px; padding: 0 12px; font: inherit; color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: var(--radius); }
  .shots { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .shots button.shoot { font-size: 16px; padding: 8px 6px; }
  button.shoot .lines { display: flex; flex-direction: column; align-items: center; min-width: 0; line-height: 1.2; }
  button.shoot small { font-size: 12px; font-weight: 500; opacity: 0.9; }
  button.shoot.armed { background: var(--danger, #e4572e); border-color: var(--danger, #e4572e); }
  button.shoot .where { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .simnote { margin: 0; padding: 6px 10px; border: 1px dashed var(--line); border-radius: var(--radius); color: var(--muted); font-size: 12px; }
  .banner { margin: 0; padding: 8px 10px; border: 1px solid var(--warn); border-radius: var(--radius); font-size: 13px; }
  .reading { display: flex; flex-wrap: wrap; gap: 8px; min-height: 72px; align-items: stretch; }
  .dock { border-top: 1px solid var(--line); padding: 8px 10px; display: flex; flex-direction: column; gap: 8px; background: var(--panel); }
  .slots { display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }
  .slots button { padding: 8px 6px; font-size: 13px; }
  .big { flex: 1 1 70px; min-width: 0; text-align: center; padding: 10px 4px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); }
  .big b { display: block; font-size: 26px; font-variant-numeric: tabular-nums; }
  .big small { color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; }
  .idle { color: var(--muted); margin: 6px 0; align-self: center; }
  .problem { margin: 0; color: var(--danger); }
  .caution { margin: 0; color: var(--warn); font-size: 13px; }
  .stoplink { display: inline-flex; align-items: center; gap: 4px; margin-left: 8px; min-height: 36px; padding: 2px 10px; font: inherit; color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: var(--radius); cursor: pointer; vertical-align: middle; }
  .stored { margin: 0; padding-left: 1.2em; font-size: 13px; font-variant-numeric: tabular-nums; }
  .hold { padding: 10px; border: 1px solid var(--accent); border-radius: var(--radius); }
  .hold p { margin: 0 0 8px; }
  .row { display: flex; flex-wrap: wrap; gap: 8px; }
  .row button { flex: 1; }
  .where { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 14px; }
  .chk { display: inline-flex; align-items: center; gap: 6px; min-height: 44px; }
  .chk input { width: 20px; height: 20px; }
  .names { display: flex; align-items: center; gap: 8px; flex: 1 1 100%; }
  .names label { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
  input.name { width: 6.5em; font: inherit; font-size: 18px; font-weight: 600; padding: 6px 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  input.name:disabled { opacity: 0.5; }
  button.link { min-height: 0; padding: 2px 8px; font-size: 12px; margin-left: 6px; }
  .wait { margin: 0; flex: 1 1 100%; font-size: 13px; color: var(--accent); min-height: 2.6em; }
  .next { margin: 0; flex: 1 1 100%; font-size: 15px; font-weight: 600; min-height: 2.6em; }
  h4 { margin: 0 0 6px; font-size: 12px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.06em; font-weight: 600; }
  .lrud { display: flex; flex-wrap: wrap; gap: 10px; }
  .lr { display: inline-flex; align-items: center; gap: 6px; }
  .lr :global(input) { font-size: 16px; padding: 8px; width: 5.5em; }
  .hint { color: var(--muted); font-size: 12px; }
  .log .entry { display: flex; justify-content: space-between; gap: 10px; padding: 4px 0; border-bottom: 1px solid var(--line); font-size: 13px; }
  .log .entry span { color: var(--muted); font-variant-numeric: tabular-nums; }
  .log .entry.flag b { color: var(--warn); }
  .link { display: flex; flex-direction: column; gap: 8px; }
  .link .opts { height: 64px; }
  .devhead { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .devhead h4 { margin: 0; }
  .tools { display: flex; flex-wrap: wrap; gap: 8px; }
  button.qr { min-height: 36px; padding: 4px 10px; font-size: 13px; flex: 1; }
  .seg { display: inline-flex; border: 1px solid var(--line); border-radius: var(--radius); overflow: hidden; }
  .seg button { border: 0; border-radius: 0; min-height: 36px; padding: 4px 14px; background: var(--panel-2); color: var(--muted); }
  .seg button.on { background: var(--accent); color: var(--on-accent, #fff); font-weight: 600; }
  .sweep.waiting { border-style: dashed; }
  .grow { flex: 1 1 100%; margin: 0; }
  .aim { font-size: 17px; font-weight: 700; color: var(--warn); }
  .acc { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--muted); }
  .acc select { flex: 1; min-width: 0; font: inherit; font-size: 13px; padding: 6px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); }
  .due { margin: 0; color: var(--warn); font-weight: 600; }
  .device { display: flex; flex-direction: column; gap: 8px; }
  .device select { font: inherit; font-size: 12px; padding: 2px 4px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); }
  button.sweepbtn { flex-direction: column; gap: 2px; min-height: 64px; font-weight: 600; }
  button.sweepbtn small { font-weight: 400; font-size: 11px; color: var(--muted); }
  .sweep { display: flex; gap: 10px; align-items: center; padding: 8px; border: 1px solid var(--warn); border-radius: var(--radius); }
  .sweep.done { border-color: var(--accent); }
  .sweepinfo { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .sweepinfo p { margin: 0; }
  .sweephead { font-size: 14px; }
  .todo { color: var(--warn); font-weight: 600; font-size: 14px; }
  .ok { color: var(--accent); font-weight: 600; display: inline-flex; align-items: center; gap: 4px; }
  .warnline { color: var(--warn); }
  .capture summary { cursor: pointer; color: var(--muted); font-size: 13px; padding: 6px 0; }
  .capture textarea { width: 100%; box-sizing: border-box; font: inherit; font-size: 12px; padding: 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); margin: 6px 0; }
  .capture pre { max-height: 200px; overflow: auto; font-size: 11px; background: var(--panel-2); padding: 6px; border-radius: var(--radius); margin: 8px 0 0; white-space: pre-wrap; word-break: break-all; }
</style>
