pragma Singleton

import Quickshell
import "lib/format.mjs" as Format
import "lib/labels.mjs" as Labels
import "lib/segments.mjs" as Segments

// Re-exports the formatters so widgets reach them through the GhostKit import instead of by file path.
Singleton {
    function pct(value, dp) {
        return Format.pct(value, dp);
    }

    function tempC(value) {
        return Format.tempC(value);
    }

    function tempShort(value) {
        return Format.tempShort(value);
    }

    function gib(kib, dp) {
        return Format.gib(kib, dp);
    }

    function rate(bps) {
        return Format.rate(bps);
    }

    function ghz(mhz) {
        return Format.ghz(mhz);
    }

    function rpm(value) {
        return Format.rpm(value);
    }

    function watts(value) {
        return Format.watts(value);
    }

    function ddmmyy(date) {
        return Format.ddmmyy(date);
    }

    function countdown(nowMs, startMs, endMs) {
        return Format.countdown(nowMs, startMs, endMs);
    }

    function uptime(seconds) {
        return Format.uptime(seconds);
    }

    function ago(seconds) {
        return Format.ago(seconds);
    }

    function utcOffset(minutesEast) {
        return Format.utcOffset(minutesEast);
    }

    function dow(date) {
        return Format.dow(date);
    }

    function cpuModelShort(model) {
        return Labels.cpuModelShort(model);
    }

    function gpuNameShort(name) {
        return Labels.gpuNameShort(name);
    }

    function ccdTitle(ccd) {
        return Labels.ccdTitle(ccd);
    }

    function reading(snapshot, id) {
        return Labels.reading(snapshot, id);
    }

    function levelAbove(value, warnAt, critAt) {
        return Segments.levelAbove(value, warnAt, critAt);
    }
}
