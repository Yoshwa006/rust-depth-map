#[inline] pub fn clamp<T: PartialOrd>(v: T, lo: T, hi: T) -> T {
    if v < lo { lo } else if v > hi { hi } else { v }
}
#[inline] pub fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
#[inline] pub fn to_u8(v: f32) -> u8 { (v.clamp(0.0, 1.0) * 255.0).round() as u8 }
