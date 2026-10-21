const CPU_VENDOR_PREFIX = /^(AMD |Intel\(R\) Core\(TM\) )/i;
const CPU_CORE_COUNT_SUFFIX = /\s+\d+-Core Processor$/i;
const GPU_VENDOR_PREFIX = /^(NVIDIA GeForce |AMD Radeon )/i;

// The shell starts before the daemon, so a model or name may not have arrived yet.
function textOrEmpty(value) {
  return typeof value === 'string' ? value : '';
}

export function cpuModelShort(model) {
  return textOrEmpty(model).trim().replace(CPU_VENDOR_PREFIX, '').replace(CPU_CORE_COUNT_SUFFIX, '').toUpperCase();
}

export function gpuNameShort(name) {
  return textOrEmpty(name).trim().replace(GPU_VENDOR_PREFIX, '');
}

// The die without the extra L3 is the one that clocks higher, hence FREQ.
export function ccdTitle(ccd) {
  return 'CCD' + ccd.id + ' · ' + (ccd.vcache ? 'V-CACHE' : 'FREQ');
}

// Ids name this machine's chips, so widgets look readings up by the daemon's labels, which a user
// maps their own sensors onto through sensors.labels.
export function readingByLabel(snapshot, label) {
  if (!snapshot) {
    return null;
  }
  const readings = (snapshot.temps || []).concat(snapshot.fans || []);
  for (let i = 0; i < readings.length; i += 1) {
    if (readings[i].label === label) {
      return readings[i];
    }
  }
  return null;
}
