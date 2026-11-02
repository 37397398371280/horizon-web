// Browsers keep sound off until the player clicks or presses a key: the
// game's audio contexts (cpal's) start suspended and are resumed then. Loaded
// before the game's glue, which keeps the AudioContext class it finds.
const contexts = [];
if (globalThis.AudioContext) {
    const BrowserAudioContext = globalThis.AudioContext;
    globalThis.AudioContext = class extends BrowserAudioContext {
        constructor(...args) {
            super(...args);
            contexts.push(this);
        }
    };
    const resume = () => {
        for (const ctx of contexts) {
            if (ctx.state === "suspended") ctx.resume().catch(() => {});
        }
    };
    addEventListener("pointerdown", resume, true);
    addEventListener("keydown", resume, true);
}

// Each context's sound goes through one gain before the speakers, so a recording can hear all of
// it (horizon-os web.rs's recorder taps `horizonAudioMasters`).
globalThis.horizonAudioMasters = [];
if (globalThis.AudioNode) {
    const connect = AudioNode.prototype.connect;
    const masters = new Map();
    AudioNode.prototype.connect = function (target, ...rest) {
        if (target instanceof AudioDestinationNode && target.context === this.context) {
            let master = masters.get(this.context);
            if (!master) {
                master = this.context.createGain();
                connect.call(master, target);
                masters.set(this.context, master);
                globalThis.horizonAudioMasters.push(master);
            }
            return connect.call(this, master, ...rest);
        }
        return connect.call(this, target, ...rest);
    };
}
