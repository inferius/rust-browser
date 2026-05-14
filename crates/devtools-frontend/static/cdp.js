// RustWebEngine DevTools CDP JS client.
//
// `window.cdp.send(method, params)` -> Promise<Result | throws Error>.
// `window.cdp.on(event, callback)` -> register event listener.
//
// Bridge na engine target adapter (D2) probiha pres native binding (D6):
// `__rwe_cdp_send_native(json_request_str)` synchronni call (same-process).
//
// Pri WebSocket transport (multi-process budouci) - wrap pres ws message
// queue + correlation by id.

(function () {
    'use strict';

    let nextId = 1;
    const pendingRequests = new Map(); // id -> { resolve, reject }
    const eventListeners = new Map(); // method -> [callback]

    function send(method, params) {
        const id = nextId++;
        const req = { id, method, params: params || {} };
        return new Promise((resolve, reject) => {
            pendingRequests.set(id, { resolve, reject });
            try {
                // Native binding (D6) - synchronni call vrati string s JSON
                // response. Engine-side dispatch pres DevtoolsTarget.
                const respJson = __rwe_cdp_send_native(JSON.stringify(req));
                handleResponseJson(respJson);
            } catch (e) {
                pendingRequests.delete(id);
                reject(e);
            }
        });
    }

    function handleResponseJson(json) {
        let resp;
        try { resp = JSON.parse(json); }
        catch (e) {
            console.error('CDP: invalid response JSON', e);
            return;
        }
        if (resp.method) {
            // Event broadcast (no id).
            dispatchEvent(resp);
        } else if (typeof resp.id === 'number') {
            // Response na request.
            const p = pendingRequests.get(resp.id);
            if (p) {
                pendingRequests.delete(resp.id);
                if (resp.error) {
                    const err = new Error(resp.error.message);
                    err.code = resp.error.code;
                    p.reject(err);
                } else {
                    p.resolve(resp.result || {});
                }
            }
        }
    }

    function dispatchEvent(evt) {
        const list = eventListeners.get(evt.method);
        if (!list) return;
        for (const cb of list) {
            try { cb(evt.params || {}); }
            catch (e) { console.error('CDP listener throw:', e); }
        }
    }

    function on(method, callback) {
        let list = eventListeners.get(method);
        if (!list) { list = []; eventListeners.set(method, list); }
        list.push(callback);
    }

    function off(method, callback) {
        const list = eventListeners.get(method);
        if (!list) return;
        const i = list.indexOf(callback);
        if (i >= 0) list.splice(i, 1);
    }

    // Periodic event poll - native binding (D6) push'uje events do
    // queue, JS volat `__rwe_cdp_poll_events()` -> JSON array.
    function pollEvents() {
        try {
            const eventsJson = __rwe_cdp_poll_events();
            if (!eventsJson) return;
            const events = JSON.parse(eventsJson);
            for (const e of events) dispatchEvent(e);
        } catch (e) {
            // Native binding chybi (pre-D6) - skip.
        }
    }

    window.cdp = { send, on, off, pollEvents };

    // Poll events 4x/s pro near-real-time event delivery.
    if (typeof setInterval === 'function') {
        setInterval(pollEvents, 250);
    }
})();
