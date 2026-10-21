const CPU_VENDOR_PREFIX = /^(AMD |Intel\(R\) Core\(TM\) )/i;
const CPU_CORE_COUNT_SUFFIX = /\s+\d+-Core Processor$/i;
const GPU_VENDOR_PREFIX = /^(NVIDIA GeForce |AMD Radeon )/i;

export function cpuModelShort(model) {
  return model.trim().replace(CPU_VENDOR_PREFIX, '').replace(CPU_CORE_COUNT_SUFFIX, '').toUpperCase();
}

export function gpuNameShort(name) {
  return name.trim().replace(GPU_VENDOR_PREFIX, '');
}

// The die without the extra L3 is the one that clocks higher, hence FREQ.
export function ccdTitle(ccd) {
  return 'CCD' + ccd.id + ' · ' + (ccd.vcache ? 'V-CACHE' : 'FREQ');
}

export function reading(snapshot, id) {
  if (!snapshot) {
    return null;
  }
  const readings = (snapshot.temps || []).concat(snapshot.fans || []);
  for (let i = 0; i < readings.length; i += 1) {
    if (readings[i].id === id) {
      return readings[i];
    }
  }
  return null;
}
