export async function request(url, body, method = 'POST') {
  const response = await fetch(url, {
    method, headers: { 'content-type': 'application/json' },
    ...(body ? { body: JSON.stringify(body) } : {}),
  }).then(result => result.json());
  // W3C driver errors are strings; an IPC result can contain a structured app error.
  if (typeof response.value?.error === 'string') throw new Error(`${response.value.error}: ${response.value.message}`);
  return response.value;
}
export function execute(peer, script, args = []) {
  return request(`${peer.url}/execute/sync`, { script, args });
}
export async function ipc(peer, command, args = {}) {
  const response = await request(`${peer.url}/execute/async`, {
    script: `const done=arguments[arguments.length-1];
      window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(
        value=>done({ok:true,value}),error=>done({ok:false,error}));`,
    args: [command, args],
  });
  if (!response.ok) throw new Error(JSON.stringify(response.error));
  return response.value;
}
export async function until(predicate, description) {
  for (let attempt = 0; attempt < 200; attempt++) {
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out: ${description}`);
}
export function click(peer, text) {
  return execute(peer, `const button=Array.from(document.querySelectorAll('button')).find(
    button=>button.textContent.includes(arguments[0]));
    if(!button||button.disabled)throw Error('Missing or disabled button');button.click();`, [text]);
}
export function input(peer, selector, value) {
  return execute(peer, `const element=document.querySelector(arguments[0]);
    Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element),'value').set.call(element,arguments[1]);
    element.dispatchEvent(new Event('input',{bubbles:true}));`, [selector, value]);
}
export async function visible(peer, text) {
  return execute(peer, 'return document.body.innerText.includes(arguments[0])', [text]);
}
