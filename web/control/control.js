// SPDX-License-Identifier: GPL-3.0-or-later
'use strict';
(() => {
  const $ = id => document.getElementById(id);
  let token = '', state = null, devices = [], busy = false, serial = 0;
  const buttons = [...document.querySelectorAll('[data-action]')];
  function message(text) { $('message').textContent = text; }
  function render() {
    const available = new Set(state?.available || []);
    for (const b of buttons) b.disabled = !token || busy || !available.has(b.dataset.action);
    $('refresh').disabled = !token || busy;
    $('unlock').disabled = busy;
    $('devices').replaceChildren();
    for (const device of devices) {
      const row = document.createElement('div'); row.className = 'device';
      const label = document.createElement('span');
      const bond = device.bonded === null ? 'bond unknown' : device.bonded ? 'bonded' : 'unbonded';
      label.textContent = `${device.name || 'Unnamed device'} · ${bond}${device.audio_sink === null ? ' · audio support unknown' : device.audio_sink ? ' · audio sink' : ''}`;
      const button = document.createElement('button'); button.textContent = device.selected ? 'Selected' : 'Select';
      button.disabled = !token || busy || device.selected || !available.has('select');
      button.addEventListener('click', () => command('select', device.id));
      row.append(label, button); $('devices').append(row);
    }
    const selected = devices.find(d => d.id === state?.selected);
    $('selected').textContent = selected ? `Selected: ${selected.name || 'Unnamed device'}` : 'No device selected';
    $('status').textContent = state ? JSON.stringify({ owner_phase: state.owner_phase, cleanup_required: state.cleanup_required, ...state.status, scanning: state.scanning, pending: state.pending, urgent: state.urgent, completion: state.completion }, null, 2) : 'Unavailable';
    $('approval').hidden = !state?.approval;
    if (state?.approval) {
      const number = state.approval.number;
      $('approval-text').textContent = number === null ? 'Confirm pairing with the selected device.' : `Confirm that the selected device shows ${String(number).padStart(6, '0')}.`;
    }
  }
  async function request(path, headers = {}, method = 'GET') {
    const response = await fetch(path, { method, headers: { Authorization: `Bearer ${token}`, ...headers }, credentials: 'omit', cache: 'no-store', redirect: 'error', signal: AbortSignal.timeout(5000) });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || `HTTP ${response.status}`);
    return result;
  }
  async function refresh() {
    const run = ++serial;
    const next = await request('/v2/status');
    const listing = await request('/v2/devices');
    if (run !== serial || !token) return;
    if (next.version !== 2 || listing.version !== 2 || next.generation !== listing.generation) throw new Error('State changed; refresh again.');
    state = next; devices = listing.devices; render();
  }
  async function command(action, device) {
    if (!state || busy) return;
    const headers = { 'X-PS5-BT-Control': '1', 'X-PS5-BT-Generation': state.generation };
    if (device) headers['X-PS5-BT-Device'] = device;
    if (action === 'approve' || action === 'reject') headers['X-PS5-BT-Approval'] = state.approval.token;
    busy = true; render();
    try { const result = await request(`/v2/${action}`, headers, 'POST'); state = null; message(`Queued command ${result.id}; awaiting owner completion.`); await refresh(); }
    catch (error) { state = null; devices = []; message(error.message); }
    finally { busy = false; render(); }
  }
  for (const button of buttons) button.addEventListener('click', () => command(button.dataset.action));
  $('unlock').addEventListener('click', async () => {
    token = $('token').value; $('token').value = ''; state = null; devices = []; busy = true; render();
    try { await refresh(); message('Connected to local control service.'); }
    catch (error) { token = ''; message(error.message); }
    finally { busy = false; render(); }
  });
  $('lock').addEventListener('click', () => { ++serial; token = ''; state = null; devices = []; $('token').value = ''; message('Locked.'); render(); });
  $('refresh').addEventListener('click', async () => { busy = true; render(); try { await refresh(); message('Updated.'); } catch (error) { state = null; devices = []; message(error.message); } finally { busy = false; render(); } });
  setInterval(async () => { if (token && !busy && !document.hidden) { busy = true; try { await refresh(); } catch (error) { state = null; devices = []; message(error.message); } finally { busy = false; render(); } } }, 2000);
  window.addEventListener('pagehide', () => { token = ''; state = null; devices = []; });
  render();
})();
