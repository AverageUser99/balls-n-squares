// The compressed performance lives inside music.wasm. Web Audio is the browser's
// required output/MP3 decoder; the scene itself has no audio or DOM dependency.
let context;
let gain;
let buffer;
let source;
let muted = false;
let loading;
let stopped = false;

function startIfReady() {
  if (stopped || !context || context.state !== 'running' || !buffer || source) return;
  source = context.createBufferSource();
  source.buffer = buffer;
  source.loop = true;
  source.connect(gain);
  source.start();
}

function initialize() {
  if (loading) return loading;
  const AudioContext = window.AudioContext || window.webkitAudioContext;
  if (!AudioContext) return Promise.reject(new Error('Web Audio is unavailable.'));
  context = new AudioContext();
  gain = context.createGain();
  gain.gain.value = muted ? 0 : 0.4;
  gain.connect(context.destination);
  loading = fetch('./music.wasm')
    .then(response => {
      if (!response.ok) throw new Error(`music.wasm: HTTP ${response.status}`);
      return response.arrayBuffer();
    })
    .then(bytes => WebAssembly.instantiate(bytes, {}))
    .then(({ instance }) => {
      const wasm = instance.exports;
      // Decode a copy: decodeAudioData can detach its argument.
      return context.decodeAudioData(new Uint8Array(wasm.memory.buffer, wasm.track_ptr(), wasm.track_len()).slice().buffer);
    })
    .then(decoded => { buffer = decoded; startIfReady(); });
  return loading;
}

function unlock() {
  if (stopped) return;
  try {
    // Call resume synchronously within the gesture; decoding may finish later.
    initialize().catch(error => console.error('Music could not be loaded:', error));
    if (context && context.state !== 'running') context.resume().then(startIfReady).catch(() => {});
    else startIfReady();
  } catch (error) { console.error('Music could not be started:', error); }
}

// First try autoplay where permitted; most browsers wait for the first gesture.
unlock();
window.addEventListener('pointerdown', unlock, { capture: true });
window.addEventListener('keydown', event => {
  if (event.code === 'KeyM' && !event.repeat && !event.altKey && !event.ctrlKey && !event.metaKey) {
    muted = !muted;
    if (gain) gain.gain.value = muted ? 0 : 0.4;
    event.preventDefault();
  }
  unlock();
}, { capture: true });
window.addEventListener('pagehide', event => {
  if (event.persisted) return;
  stopped = true;
  if (source) source.stop();
  if (context) context.close().catch(() => {});
});
