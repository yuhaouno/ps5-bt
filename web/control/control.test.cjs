// SPDX-License-Identifier: GPL-3.0-or-later
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
class Element {
  constructor() { this.textContent = ''; this.value = ''; this.disabled = false; this.hidden = false; this.dataset = {}; this.children = []; this.listeners = {}; }
  set innerHTML(_) { throw new Error('unsafe HTML insertion'); }
  addEventListener(name, listener) { this.listeners[name] = listener; }
  replaceChildren(...children) { this.children = children; }
  append(...children) { this.children.push(...children); }
  async click() { if (!this.disabled) await this.listeners.click?.(); }
}
function setup(fetch) {
  const ids = Object.fromEntries(['token','unlock','lock','refresh','message','devices','selected','status','approval','approval-text'].map(id => [id, new Element()]));
  const actions = ['scan','cancel-scan','pair','connect','forget','approve','reject','start','stop','disconnect'].map(action => { const element = new Element(); element.dataset.action = action; return element; });
  const calls = [], timers = [];
  vm.runInNewContext(fs.readFileSync(__dirname + '/control.js', 'utf8'), {
    document: { hidden: false, getElementById: id => ids[id], querySelectorAll: () => actions, createElement: () => new Element() },
    window: { addEventListener() {} }, AbortSignal,
    setInterval: callback => timers.push(callback),
    fetch: async (path, options) => { calls.push({path,options}); return fetch(path,options); },
  });
  return {ids,actions,calls,timers};
}
function response(value,ok=true) { return {ok,status:ok?200:409,json:async()=>value}; }
function state(generation='19:1') { return {version:2,generation,status:{stream:'stopped'},owner_phase:'connected',cleanup_required:false,scanning:false,selected:'1',approval:{token:'9007199254740993',device:'1',number:42},available:['scan','select','approve','reject'],pending:null,urgent:null,completion:null}; }
function listing(generation='19:1') { return {version:2,generation,devices:[{id:'1',name:'<img src=x onerror=alert(1)>',bonded:null,audio_sink:null,selected:true},{id:'18446744073709551615',name:'Other',bonded:false,audio_sink:null,selected:false}]}; }
(async()=>{
  let generation='19:1';
  const ui=setup(async(path,options)=> {
    if (options.method==='POST') { generation='19:2'; return response({queued:true,id:'1'}); }
    return response(path==='/v2/status'?state(generation):listing(generation));
  });
  assert(ui.actions.every(button=>button.disabled));
  ui.ids.token.value='secret-never-store-or-url';await ui.ids.unlock.click();
  assert.equal(ui.ids.token.value,'');assert.equal(ui.ids.devices.children[0].children[0].textContent,'<img src=x onerror=alert(1)> · bond unknown · audio support unknown');
  assert(ui.ids['approval-text'].textContent.includes('000042'));
  assert.equal(ui.actions.find(b=>b.dataset.action==='start').disabled,true);
  await ui.actions.find(b=>b.dataset.action==='approve').click();
  const post=ui.calls.find(call=>call.options.method==='POST');
  assert.equal(post.path,'/v2/approve');assert.equal(post.options.headers['X-PS5-BT-Generation'],'19:1');assert.equal(post.options.headers['X-PS5-BT-Approval'],'9007199254740993');
  assert(ui.calls.every(call=>!call.path.includes('secret')&&!call.path.includes('?')));
  assert(ui.calls.every(call=>call.options.headers.Authorization==='Bearer secret-never-store-or-url'));
  await ui.ids.lock.click();const count=ui.calls.length;await ui.timers[0]();assert.equal(ui.calls.length,count);assert(ui.actions.every(button=>button.disabled));

  const stale=setup(async path=>response(path==='/v2/status'?state('1:1'):listing('1:2')));
  stale.ids.token.value='temporary';await stale.ids.unlock.click();assert(stale.ids.message.textContent.includes('State changed'));assert(stale.actions.every(button=>button.disabled));assert.equal(stale.ids.token.value,'');

  const denied=setup(async()=>response({error:'unauthorized'},false));denied.ids.token.value='wrong';await denied.ids.unlock.click();assert.equal(denied.ids.message.textContent,'unauthorized');assert(denied.actions.every(button=>button.disabled));
  console.log('3 browser-controller scenarios passed (mock DOM/fetch; no browser or hardware claim)');
})().catch(error=>{ console.error(error); process.exitCode=1; });
