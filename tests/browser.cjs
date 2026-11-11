const { spawn } = require('node:child_process');
const { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');
const net = require('node:net');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '..');
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function freePort() {
  const server = net.createServer();
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  const port = server.address().port;
  await new Promise(r => server.close(r));
  return port;
}

(async () => {
  const profile = mkdtempSync(path.join(tmpdir(), 'wasm-scene-browser-'));
  const port = await freePort();
  const server = spawn('python', ['-m', 'http.server', String(port), '--bind', '127.0.0.1'], { cwd: root, stdio: 'ignore' });
  let browser;
  let socket;
  try {
    for (let attempt = 0; ; attempt++) {
      try { if ((await fetch(`http://127.0.0.1:${port}/`)).ok) break; } catch {}
      if (attempt > 100) throw new Error('Python server did not start');
      await sleep(50);
    }
    const executable = [process.env.BROWSER_PATH,
      'C:/Program Files/Google/Chrome/Application/chrome.exe',
      'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
      '/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome',
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    ].find(file => file && existsSync(file));
    if (!executable) throw new Error('Set BROWSER_PATH to a Chromium-based browser executable.');
    browser = spawn(executable, [
      '--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`,
      '--no-first-run', '--no-default-browser-check', '--disable-background-networking',
      '--disable-gpu', 'about:blank',
    ], { stdio: ['ignore', 'ignore', 'pipe'] });
    const endpoint = await new Promise((resolve, reject) => {
      let output = '';
      const timer = setTimeout(() => reject(new Error('Chrome CDP startup timed out: ' + output)), 15000);
      browser.stderr.on('data', data => {
        output += data;
        const match = output.match(/DevTools listening on (ws:\/\/[^\s]+)/);
        if (match) { clearTimeout(timer); resolve(match[1]); }
      });
      browser.on('error', reject);
    });
    socket = new WebSocket(endpoint);
    await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
    let id = 0;
    const pending = new Map();
    const errors = [];
    socket.onmessage = event => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const p = pending.get(message.id);
        pending.delete(message.id);
        if (message.error) p.reject(new Error(JSON.stringify(message.error)));
        else p.resolve(message.result);
      } else if (message.method === 'Fetch.requestPaused') {
        call('Fetch.fulfillRequest', { requestId: message.params.requestId, responseCode: 404, body: '' }, message.sessionId)
          .catch(error => errors.push(String(error)));
      } else if (message.method === 'Runtime.exceptionThrown') {
        errors.push(message.params.exceptionDetails);
      } else if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
        errors.push(message.params.args);
      }
    };
    function call(method, params = {}, sessionId) {
      return new Promise((resolve, reject) => {
        const request = { id: ++id, method, params };
        if (sessionId) request.sessionId = sessionId;
        pending.set(id, { resolve, reject });
        socket.send(JSON.stringify(request));
      });
    }
    const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
    const page = (method, params) => call(method, params, sessionId);
    await page('Runtime.enable');
    await page('Page.enable');
    await page('Page.addScriptToEvaluateOnNewDocument', { source: `
      window.__audioStarted = null;
      const original = AudioBufferSourceNode.prototype.start;
      AudioBufferSourceNode.prototype.start = function(...args) {
        window.__audioStarted = { loop: this.loop, duration: this.buffer?.duration, context: this.context };
        return original.apply(this, args);
      };
    ` });
    await page('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
    if (process.env.TEST_RUNTIME_BAKE) {
      await page('Fetch.enable', { patterns: [{ urlPattern: '*lighting.bin' }] });
    }
    const navigationStart = performance.now();
    await page('Page.navigate', { url: `http://127.0.0.1:${port}/` });
    const statsCode = `(() => {
      const c = document.querySelector('canvas');
      const p = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
      let lit = 0;
      for (let i = 0; i < p.length; i += 4) if (Math.max(p[i], p[i+1], p[i+2]) > 40) lit++;
      return { width: c.width, height: c.height, litFraction: lit / (p.length / 4), text: document.body.innerText.trim() };
    })()`;
    const evaluate = async expression => (await page('Runtime.evaluate', { expression, returnByValue: true })).result.value;
    let landscape;
    do {
      await sleep(100);
      landscape = await evaluate(statsCode);
    } while ((!landscape || landscape.litFraction <= 0.9) && performance.now() - navigationStart < 30000);
    console.log('First lit frame after', Math.round(performance.now() - navigationStart), 'ms');
    await sleep(1000);
    landscape = await evaluate(statsCode);
    console.log('Landscape:', landscape);
    assert.ok(landscape.litFraction > 0.9, JSON.stringify(errors));
    assert.equal(await evaluate('crossOriginIsolated'), false, 'plain Python server must not need isolation headers');
    const { targetInfos } = await call('Target.getTargets');
    const workerCount = targetInfos.filter(target => target.type === 'worker').length;
    assert.ok(workerCount >= 1);
    console.log('Live WASM workers:', workerCount);
    assert.match(landscape.text, /M\s+Mute\s+C\s+Camera\s+R\s+Reset/);
    // Two short real pointer drags; leave visual/feel tuning to user testing.
    for (const [x, y, toX, toY] of [[510, 540, 420, 555], [760, 540, 840, 555]]) {
      await page('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      await page('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 });
      assert.equal(await evaluate("document.querySelector('canvas').style.cursor"), 'grabbing');
      for (let step = 1; step <= 4; step++) {
        await page('Input.dispatchMouseEvent', { type: 'mouseMoved', x: x + (toX - x) * step / 4, y: y + (toY - y) * step / 4, buttons: 1 });
        await sleep(60);
      }
      await page('Input.dispatchMouseEvent', { type: 'mouseReleased', x: toX, y: toY, button: 'left', buttons: 0, clickCount: 1 });
      assert.equal(await evaluate("document.querySelector('canvas').style.cursor"), 'grab');
      await sleep(300);
    }
    console.log('Sphere and cube pointer drags passed.');
    const audioDeadline = performance.now() + 15000;
    let audio;
    do {
      await sleep(100);
      audio = await evaluate('window.__audioStarted && ({loop: window.__audioStarted.loop, duration: window.__audioStarted.duration, state: window.__audioStarted.context.state})');
    } while (!audio && performance.now() < audioDeadline);
    assert.ok(audio && audio.loop && audio.duration > 240 && audio.state === 'running', `Music did not start: ${JSON.stringify(audio)} ${JSON.stringify(errors)}`);
    console.log('Looping music started:', audio);
    const screenshot = await page('Page.captureScreenshot', { format: 'png' });
    mkdirSync(path.join(root, 'target'), { recursive: true });
    writeFileSync(path.join(root, 'target', 'scene.png'), Buffer.from(screenshot.data, 'base64'));
    await page('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
    await sleep(2000);
    const portrait = await evaluate(statsCode);
    console.log('Portrait:', portrait);
    assert.ok(portrait.height > portrait.width);
    assert.ok(portrait.litFraction > 0.9);
    assert.match(portrait.text, /M\s+Mute\s+C\s+Camera\s+R\s+Reset/);
    assert.deepEqual(errors, []);
    console.log('Actual Chrome + Python HTTP smoke test passed; screenshot: target/scene.png');
    await call('Browser.close');
  } finally {
    if (socket) socket.close();
    if (browser && browser.exitCode === null) browser.kill();
    server.kill();
    await sleep(500);
    try { rmSync(profile, { recursive: true, force: true }); } catch {}
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
