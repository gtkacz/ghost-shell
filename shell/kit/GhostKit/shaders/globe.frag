#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

// The reference mockup's drawGlobe(), less the labels: the outline, the dots, and the arcs from home with their packets,
// end squares and the home pulse (docs/superpowers/notes/2026-10-21-globe-shader-spike.md). Lengths are logical pixels;
// colours arrive premultiplied, which is also what fragColor must be. lib/globe.mjs names every constant here.
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 itemSize;
    float pixelRatio;
    float unit;
    float radius;
    float dotSize;
    float rot;
    float tilt;
    float pointCount;
    float elapsedMs;
    float arcWidth;
    vec4 signalColor;
    vec4 hiColor;
    float landAlphaFloor;
    float landAlphaSpan;
    float seaAlphaSpan;
    float ringAlpha;
    float arcAlpha;
    // World unit vectors, w = 1 when present. ShaderEffect ignores array uniforms, hence named members.
    vec4 home;
    vec4 endpoint0; vec4 endpoint1; vec4 endpoint2; vec4 endpoint3;
    vec4 endpoint4; vec4 endpoint5; vec4 endpoint6; vec4 endpoint7;
    // lib/landbits.mjs: bit k of component c is the land bit of dot 24c + k.
    vec4 landWord0; vec4 landWord1; vec4 landWord2; vec4 landWord3; vec4 landWord4; vec4 landWord5;
    vec4 landWord6; vec4 landWord7; vec4 landWord8; vec4 landWord9; vec4 landWord10; vec4 landWord11;
    vec4 landWord12; vec4 landWord13; vec4 landWord14; vec4 landWord15; vec4 landWord16; vec4 landWord17;
    vec4 landWord18; vec4 landWord19; vec4 landWord20; vec4 landWord21; vec4 landWord22; vec4 landWord23;
    vec4 landWord24; vec4 landWord25; vec4 landWord26; vec4 landWord27; vec4 landWord28; vec4 landWord29;
    vec4 landWord30; vec4 landWord31; vec4 landWord32; vec4 landWord33;
};

const float PI = 3.14159265358979;
const float GOLDEN = 0.61803398874989485;
// frac(64 * GOLDEN): an index splits as 64a + b, so frac(i * GOLDEN) never multiplies a large index by GOLDEN, which
// in 32-bit floats would cost a whole dot's width of angle at the last points.
const float GOLDEN_64_FRAC = 0.55417527999327;
const float SPLIT = 64.0;
const float SQRT5 = 2.2360679774997897;
const float LOG_PHI_SQUARED = 0.96242365011920689;
const float BITS_PER_COMPONENT = 24.0;
const float COMPONENTS_PER_WORD = 4.0;
// Within this many dots of the limb, a dot two lattice cells away radially can still cover the pixel.
const float LIMB_BAND_DOTS = 4.0;
const int LIMB_LOOKUPS = 5;
const int CANDIDATES = 5;

// As lib/globe.mjs names them.
const float RING_INSET = 1.0;
const float RING_WIDTH = 0.8;
const float SEA_DOT_SCALE = 0.5;
const int ARC_SEGMENTS = 40;
const float ARC_LIFT_BASE = 0.08;
const float ARC_LIFT_SPAN = 0.22;
const float PACKET_PER_MS = 0.00025;
const float PACKET_STAGGER = 0.37;
const float PACKET = 3.0;
const float END = 4.0;
const float HOME = 5.0;
const float PULSE_MS = 1800.0;
const float PULSE_FROM = 6.0;
const float PULSE_GROW = 16.0;
const float PULSE_LINE = 1.0;
const int MAX_ENDPOINTS = 8;

float fracGolden(float i) {
    float a = floor(i / SPLIT);
    float b = i - a * SPLIT;
    return fract(a * GOLDEN_64_FRAC + b * GOLDEN);
}

// Point i of the mockup's lattice: y from 1 down to -1 in N - 1 equal steps, longitude advancing by the golden angle.
vec3 latticePoint(float i) {
    float y = 1.0 - 2.0 * i / (pointCount - 1.0);
    float r = sqrt(max(0.0, 1.0 - y * y));
    // The golden angle is 2π(2 - Φ), and frac(i(2 - Φ)) = 1 - frac(i(Φ - 1)).
    float theta = 2.0 * PI * (1.0 - fracGolden(i));
    return vec3(r * sin(theta), y, r * cos(theta));
}

// The words are separate members, so one is picked by comparison, as blockgraph.frag picks its own.
vec4 landWordAt(int i) {
    if (i < 17) {
        if (i < 8) {
            if (i == 0) { return landWord0; } if (i == 1) { return landWord1; } if (i == 2) { return landWord2; }
            if (i == 3) { return landWord3; } if (i == 4) { return landWord4; } if (i == 5) { return landWord5; }
            if (i == 6) { return landWord6; } return landWord7;
        }
        if (i == 8) { return landWord8; } if (i == 9) { return landWord9; } if (i == 10) { return landWord10; }
        if (i == 11) { return landWord11; } if (i == 12) { return landWord12; } if (i == 13) { return landWord13; }
        if (i == 14) { return landWord14; } if (i == 15) { return landWord15; } return landWord16;
    }
    if (i < 25) {
        if (i == 17) { return landWord17; } if (i == 18) { return landWord18; } if (i == 19) { return landWord19; }
        if (i == 20) { return landWord20; } if (i == 21) { return landWord21; } if (i == 22) { return landWord22; }
        if (i == 23) { return landWord23; } return landWord24;
    }
    if (i == 25) { return landWord25; } if (i == 26) { return landWord26; } if (i == 27) { return landWord27; }
    if (i == 28) { return landWord28; } if (i == 29) { return landWord29; } if (i == 30) { return landWord30; }
    if (i == 31) { return landWord31; } if (i == 32) { return landWord32; } return landWord33;
}

float component(vec4 word, int i) {
    if (i == 0) {
        return word.x;
    }
    if (i == 1) {
        return word.y;
    }
    if (i == 2) {
        return word.z;
    }
    return word.w;
}

// A component is an integer below 2^24, which a 32-bit float holds exactly, and it is divided only by powers of two,
// which is exact. 24 is not one, so the index divides plus a half: the quotient stays at least 1/48 from a whole
// number, and no rounding error can move its floor.
bool landAt(float i) {
    float c = floor((i + 0.5) / BITS_PER_COMPONENT);
    float word = floor(c / COMPONENTS_PER_WORD);
    float packed = component(landWordAt(int(word)), int(c - word * COMPONENTS_PER_WORD));
    return mod(floor(packed / exp2(i - c * BITS_PER_COMPONENT)), 2.0) == 1.0;
}

vec3 toView(vec3 v) {
    float cr = cos(rot);
    float sr = sin(rot);
    float ct = cos(tilt);
    float st = sin(tilt);
    float x = v.x * cr + v.z * sr;
    float z = v.z * cr - v.x * sr;
    return vec3(x, v.y * ct - z * st, v.y * st + z * ct);
}

vec3 fromView(vec3 q) {
    float cr = cos(rot);
    float sr = sin(rot);
    float ct = cos(tilt);
    float st = sin(tilt);
    float y = q.y * ct + q.z * st;
    float z = q.z * ct - q.y * st;
    return vec3(q.x * cr - z * sr, y, q.x * sr + z * cr);
}

vec2 toScreen(vec3 q) {
    return itemSize * 0.5 + radius * vec2(q.x, -q.y);
}

// How much of this pixel a square covers: the canvas's own antialiasing, as a box filter over the physical pixel.
float squareCoverage(vec2 p, vec2 centre, float halfSize) {
    float h = 0.5 / pixelRatio;
    vec2 overlap = clamp(min(p + h, centre + halfSize) - max(p - h, centre - halfSize), 0.0, 2.0 * h);
    return overlap.x * overlap.y / (4.0 * h * h);
}

// The same for a band of the given half width around distance zero, as a stroke is.
float bandCoverage(float distance, float halfWidth) {
    float h = 0.5 / pixelRatio;
    return clamp(min(distance + h, halfWidth) - max(distance - h, -halfWidth), 0.0, 2.0 * h) / (2.0 * h);
}

vec4 over(vec4 src, vec4 dst) {
    return src + dst * (1.0 - src.a);
}

vec4 dotColor(vec2 p, float i) {
    vec3 q = toView(latticePoint(i));
    if (q.z <= 0.0) {
        return vec4(0.0);
    }
    bool land = landAt(i);
    float halfSize = dotSize * 0.5 * (land ? 1.0 : SEA_DOT_SCALE);
    float alpha = land ? landAlphaFloor + landAlphaSpan * q.z : seaAlphaSpan * q.z;
    return signalColor * (alpha * squareCoverage(p, toScreen(q), halfSize));
}

// The view-space point under a position given in radii from the centre, or the limb's point past the limb.
vec3 hitPoint(vec2 fromCentre) {
    float r = length(fromCentre);
    vec3 q = r < 1.0 ? vec3(fromCentre.x, -fromCentre.y, sqrt(1.0 - r * r)) : vec3(fromCentre.x, -fromCentre.y, 0.0) / r;
    return fromView(q);
}

// The index of corner s of lattice cell (cu, cv), whose basis has z rows b10 and b11, or for s = 4 the pole on y's side.
float candidate(int s, float cu, float cv, float b10, float b11, float dz, float y) {
    if (s == CANDIDATES - 1) {
        return y > 0.0 ? 0.0 : pointCount - 1.0;
    }
    float u = cu + mod(float(s), 2.0);
    float w = cv + floor(float(s) / 2.0);
    float zz = b10 * u + b11 * w + 1.0;
    zz = clamp(zz, -1.0, 1.0) * 2.0 - zz;
    return clamp(floor((1.0 - zz) / dz + 0.5), 0.0, pointCount - 1.0);
}

// Keinert et al. 2015, "Spherical Fibonacci Mapping", adapted to the mockup's lattice (points on both poles, y steps of
// 2 / (N - 1)): the four lattice points around the point under the pixel, plus the nearer pole, which the four miss
// within a dot of it. Near the limb the sphere is foreshortened, and a dot whose square covers the pixel can sit two
// lattice cells further in or out, so there the lookup is repeated a quarter and a half dot either way.
vec4 dots(vec2 p, vec2 fromCentre) {
    float n = pointCount;
    float dz = 2.0 / (n - 1.0);
    float fromCentrePx = length(fromCentre) * radius;
    int lookups = fromCentrePx > radius - LIMB_BAND_DOTS * dotSize ? LIMB_LOOKUPS : 1;
    vec4 color = vec4(0.0);
    // Neighbouring corners, and the lookups near the limb, can name one point more than once.
    float seen[LIMB_LOOKUPS * CANDIDATES];
    for (int l = 0; l < LIMB_LOOKUPS; l++) {
        if (l >= lookups) {
            break;
        }
        // 0, then a quarter dot in, a quarter out, a half in and a half out.
        float inwards = (mod(float(l), 2.0) == 1.0 ? 1.0 : -1.0) * floor((float(l) + 1.0) / 2.0) * dotSize * 0.25;
        vec3 v = hitPoint(fromCentre * max(0.0, fromCentrePx - inwards) / max(fromCentrePx, 1e-6));
        // In the paper's frame, z is the mockup's y and the longitude runs the other way.
        float phi = atan(-v.x, v.z);
        float z = v.y;
        float k = max(2.0, floor(log(n * PI * SQRT5 * max(1.0 - z * z, 1e-12)) / LOG_PHI_SQUARED));
        float fk = pow(GOLDEN + 1.0, k) / SQRT5;
        float f0 = floor(fk + 0.5);
        float f1 = floor(fk * (GOLDEN + 1.0) + 0.5);
        float b00 = 2.0 * PI * (fracGolden(f0 + 1.0) - GOLDEN);
        float b01 = 2.0 * PI * (fracGolden(f1 + 1.0) - GOLDEN);
        float b10 = -f0 * dz;
        float b11 = -f1 * dz;
        float det = b00 * b11 - b01 * b10;
        float cu = floor((b11 * phi - b01 * (z - 1.0)) / det);
        float cv = floor((b00 * (z - 1.0) - b10 * phi) / det);
        for (int s = 0; s < CANDIDATES; s++) {
            float i = candidate(s, cu, cv, b10, b11, dz, z);
            int slot = l * CANDIDATES + s;
            bool repeated = false;
            for (int j = 0; j < LIMB_LOOKUPS * CANDIDATES; j++) {
                if (j < slot && seen[j] == i) {
                    repeated = true;
                }
            }
            seen[slot] = i;
            if (!repeated) {
                color = over(dotColor(p, i), color);
            }
        }
    }
    return color;
}

vec4 endpointAt(int k) {
    if (k == 0) { return endpoint0; } if (k == 1) { return endpoint1; } if (k == 2) { return endpoint2; }
    if (k == 3) { return endpoint3; } if (k == 4) { return endpoint4; } if (k == 5) { return endpoint5; }
    if (k == 6) { return endpoint6; } return endpoint7;
}

// The mockup's rule: a point is drawn while it faces the viewer, or while it is outside the disc, as a raised arc is.
bool visibleAt(vec3 q) {
    return q.z > 0.0 || dot(q.xy, q.xy) > 1.0;
}

float segmentDistance(vec2 p, vec2 a, vec2 b) {
    vec2 ab = b - a;
    float t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return length(p - a - ab * t);
}

vec4 endSquare(vec2 p, vec3 q, vec4 color) {
    if (!visibleAt(q)) {
        return color;
    }
    return over(hiColor * squareCoverage(p, toScreen(q), END * unit * 0.5), color);
}

// One arc from home to endpoint k, with its packet and end square, over `color`. The segment points come from a
// rotation recurrence, so the 41 of them cost one sin and cos pair per arc rather than one per point.
vec4 arc(vec2 p, int k, vec3 e, vec4 color) {
    vec3 h = home.xyz;
    float cosOmega = clamp(dot(h, e), -1.0, 1.0);
    float omega = acos(cosOmega);
    float lift = ARC_LIFT_BASE + ARC_LIFT_SPAN * omega / PI;
    float top = 1.0 + lift;
    // Every point of the arc is within this circle: a point a·d, with d within ω/2 of the middle direction m and
    // a between 1 and the top altitude A, is at most 2A·sin(ω/4) + A - 1 from A·m, and the projection shrinks nothing.
    vec3 middle = h + e;
    float margin = PACKET * unit + arcWidth;
    if (dot(middle, middle) > 1e-6) {
        vec2 centre = toScreen(toView(normalize(middle) * top));
        float reach = radius * (2.0 * top * sin(omega * 0.25) + top - 1.0) + margin;
        vec2 off = p - centre;
        if (dot(off, off) > reach * reach) {
            return color;
        }
    }
    vec3 across = e - h * cosOmega;
    float acrossLength = length(across);
    across = acrossLength > 1e-6 ? across / acrossLength : vec3(0.0);
    float step = omega / float(ARC_SEGMENTS);
    float cosStep = cos(step);
    float sinStep = sin(step);
    float cosLift = cos(PI / float(ARC_SEGMENTS));
    float sinLift = sin(PI / float(ARC_SEGMENTS));
    float packetIndex = floor(fract(elapsedMs * PACKET_PER_MS + float(k) * PACKET_STAGGER) * float(ARC_SEGMENTS));

    float ca = 1.0;
    float sa = 0.0;
    float cl = 1.0;
    float sl = 0.0;
    float stroke = 0.0;
    vec2 previous = vec2(0.0);
    bool previousVisible = false;
    vec2 packet = vec2(0.0);
    bool packetVisible = false;
    vec2 end = vec2(0.0);
    bool endVisible = false;
    for (int i = 0; i <= ARC_SEGMENTS; i++) {
        vec3 q = toView((h * ca + across * sa) * (1.0 + lift * sl));
        vec2 s = toScreen(q);
        bool visible = visibleAt(q);
        if (visible && previousVisible) {
            stroke = max(stroke, bandCoverage(segmentDistance(p, previous, s), arcWidth * 0.5));
        }
        if (float(i) == packetIndex) {
            packet = s;
            packetVisible = visible;
        }
        if (i == ARC_SEGMENTS) {
            end = s;
            endVisible = visible;
        }
        previous = s;
        previousVisible = visible;
        float nextCa = ca * cosStep - sa * sinStep;
        sa = sa * cosStep + ca * sinStep;
        ca = nextCa;
        float nextCl = cl * cosLift - sl * sinLift;
        sl = sl * cosLift + cl * sinLift;
        cl = nextCl;
    }
    color = over(hiColor * (arcAlpha * stroke), color);
    if (packetVisible) {
        color = over(hiColor * squareCoverage(p, packet, PACKET * unit * 0.5), color);
    }
    if (endVisible) {
        color = over(hiColor * squareCoverage(p, end, END * unit * 0.5), color);
    }
    return color;
}

// strokeRect centres its line on the square's edge, so the pulse is the band between two squares.
vec4 homeMarker(vec2 p, vec4 color) {
    vec3 q = toView(home.xyz);
    if (!visibleAt(q)) {
        return color;
    }
    vec2 s = toScreen(q);
    float pulse = mod(elapsedMs, PULSE_MS) / PULSE_MS;
    float halfSize = (PULSE_FROM + PULSE_GROW * pulse) * unit * 0.5;
    float line = PULSE_LINE * unit * 0.5;
    float outline = squareCoverage(p, s, halfSize + line) - squareCoverage(p, s, halfSize - line);
    color = over(signalColor * ((1.0 - pulse) * outline), color);
    return over(signalColor * squareCoverage(p, s, HOME * unit * 0.5), color);
}

void main() {
    vec2 p = qt_TexCoord0 * itemSize;
    vec2 fromCentre = (p - itemSize * 0.5) / radius;
    float r = length(fromCentre);
    vec4 color = signalColor * (ringAlpha * bandCoverage(r * radius - (radius + RING_INSET * unit), RING_WIDTH * unit * 0.5));
    // A dot near the limb reaches past it, so a pixel just outside looks up the dots at the limb.
    if (r * radius < radius + dotSize) {
        color = over(dots(p, fromCentre), color);
    }
    // Without a home there is nothing to draw an arc from, but the endpoints still show where they are.
    bool hasHome = home.w > 0.5;
    for (int k = 0; k < MAX_ENDPOINTS; k++) {
        vec4 e = endpointAt(k);
        if (e.w > 0.5) {
            color = hasHome ? arc(p, k, e.xyz, color) : endSquare(p, toView(e.xyz), color);
        }
    }
    if (hasHome) {
        color = homeMarker(p, color);
    }
    fragColor = color * qt_Opacity;
}
