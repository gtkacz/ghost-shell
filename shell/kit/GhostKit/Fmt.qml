pragma Singleton

import Quickshell
import "lib/format.mjs" as Format
import "lib/labels.mjs" as Labels

// Re-exports the formatters so widgets reach them through the GhostKit import instead of by file path.
Singleton {
    function pct(value) {
        return Format.pct(value);
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

    function ddmmyy(date) {
        return Format.ddmmyy(date);
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
}
