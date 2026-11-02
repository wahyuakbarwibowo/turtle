/*
 * TurtleMeet embed SDK. No dependencies; load with a <script> tag or bundle it.
 *
 *   const meet = new TurtleMeetEmbed(document.getElementById("call"), {
 *     url: embedUrlFromYourBackend,   // https://meet.example.com/embed/<code>#t=<token>
 *     audio: true, video: true,       // start with mic/camera on (default true)
 *   });
 *   meet.on("joined", (e) => ...);    // ready, joined, left, participantJoined, participantLeft,
 *                                     // audioMuteStatusChanged, videoMuteStatusChanged,
 *                                     // readyToClose, error
 *   meet.execute("toggleAudio");      // hangup, toggleAudio, toggleVideo, toggleShareScreen
 *   meet.dispose();
 */
(function (root) {
  "use strict";

  class TurtleMeetEmbed {
    constructor(container, options) {
      if (!container || !options || !options.url) {
        throw new Error("TurtleMeetEmbed: container and options.url are required");
      }
      const url = new URL(options.url, window.location.href);
      if (options.audio === false) url.searchParams.set("audio", "0");
      if (options.video === false) url.searchParams.set("video", "0");

      this._origin = url.origin;
      this._handlers = {};
      this.iframe = document.createElement("iframe");
      this.iframe.src = url.toString();
      this.iframe.allow = "camera; microphone; display-capture; autoplay; clipboard-write; fullscreen";
      this.iframe.style.cssText = "border:0;width:100%;height:100%;" + (options.style || "");
      this.iframe.title = options.title || "Video call";

      this._onMessage = (e) => {
        if (e.source !== this.iframe.contentWindow || e.origin !== this._origin) return;
        const d = e.data;
        if (!d || d.source !== "turtlemeet") return;
        // The iframe can't trust its parent's origin until we say hello.
        if (d.type === "ready") this._post({ type: "hello" });
        (this._handlers[d.type] || []).slice().forEach((fn) => fn(d));
      };
      window.addEventListener("message", this._onMessage);
      container.appendChild(this.iframe);
    }

    _post(msg) {
      if (this.iframe.contentWindow) {
        this.iframe.contentWindow.postMessage({ ...msg, source: "turtlemeet" }, this._origin);
      }
    }

    on(type, fn) {
      (this._handlers[type] = this._handlers[type] || []).push(fn);
      return this;
    }

    off(type, fn) {
      this._handlers[type] = (this._handlers[type] || []).filter((h) => h !== fn);
      return this;
    }

    execute(name) {
      this._post({ type: "command", name });
    }

    dispose() {
      window.removeEventListener("message", this._onMessage);
      this.iframe.remove();
    }
  }

  if (typeof module === "object" && module.exports) module.exports = TurtleMeetEmbed;
  else root.TurtleMeetEmbed = TurtleMeetEmbed;
})(typeof self !== "undefined" ? self : this);
