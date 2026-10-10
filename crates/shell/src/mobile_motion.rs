//! Shared, time-based motion for Home surfaces. Coordinates are logical points
//! (pages for the pager); no platform View or frame-rate assumption is involved.

/// Exact damped spring solution: splitting a frame does not change its path.
pub fn spring(x: &mut f64, v: &mut f64, target: f64, dt: f64, stiffness: f64, damping: f64) {
    let dt = dt.max(0.0);
    let w = stiffness.sqrt();
    let y = *x - target;
    if damping >= 1.0 {
        let b = *v + w * y;
        let decay = (-w * dt).exp();
        *x = target + (y + b * dt) * decay;
        *v = (*v - w * b * dt) * decay;
    } else {
        let a = w * damping;
        let b = w * (1.0 - damping * damping).sqrt();
        let (s, c) = (b * dt).sin_cos();
        let decay = (-a * dt).exp();
        *x = target + decay * (y * c + (*v + a * y) / b * s);
        *v = decay * (*v * c - (a * *v + stiffness * y) / b * s);
    }
}

const EDGE_LIMIT: f64 = 96.0;
const EDGE_GAIN: f64 = 0.55;

/// Progressively stiffens without an abrupt clamp. Its inverse lets a finger
/// grab an in-flight rebound and reverse without changing the drawn position.
pub fn rubber(distance: f64) -> f64 {
    distance.signum() * EDGE_LIMIT * (1.0 - 1.0 / (1.0 + distance.abs() * EDGE_GAIN / EDGE_LIMIT))
}
fn unrubber(stretch: f64) -> f64 {
    let s = stretch.abs().min(EDGE_LIMIT - 0.001);
    stretch.signum() * s / (EDGE_GAIN * (1.0 - s / EDGE_LIMIT))
}

pub fn drag(offset: &mut f64, stretch: &mut f64, delta: f64, max: f64) {
    let max = max.max(0.0);
    let raw = offset.clamp(0.0, max) - unrubber(*stretch) + delta;
    *offset = raw.clamp(0.0, max);
    *stretch = rubber(*offset - raw);
}

/// Convert finger velocity to the actual stretched surface's velocity.
pub fn stretch_velocity(stretch: f64, finger_velocity: f64) -> f64 {
    finger_velocity.clamp(-4000.0, 4000.0) * EDGE_GAIN * (1.0 - stretch.abs() / EDGE_LIMIT).max(0.0).powi(2)
}

/// In-bounds velocity increases scroll offset; positive stretch pulls content
/// down. A fling transfers its velocity to the edge spring at collision time.
pub fn scroll(offset: &mut f64, velocity: &mut f64, stretch: &mut f64, edge_velocity: &mut f64,
    max: f64, dt: f64, friction: f64, reduced: bool) -> bool {
    let max = max.max(0.0);
    *offset = offset.clamp(0.0, max);
    if reduced { *stretch = 0.0; *edge_velocity = 0.0; }
    let mut edge_dt = dt.max(0.0);
    if *stretch == 0.0 && *edge_velocity == 0.0 && *velocity != 0.0 {
        let decay = (-friction * edge_dt).exp();
        let next = *offset + *velocity * (1.0 - decay) / friction;
        if next < 0.0 || next > max {
            let end = next.clamp(0.0, max);
            let hit_decay = (1.0 - (end - *offset) * friction / *velocity).clamp(1e-9, 1.0);
            edge_dt = (edge_dt + hit_decay.ln() / friction).max(0.0);
            *edge_velocity = if reduced { 0.0 } else { -*velocity * hit_decay };
            *velocity = 0.0;
            *offset = end;
        } else {
            *offset = next;
            *velocity *= decay;
            if velocity.abs() < 20.0 { *velocity = 0.0; }
            return *velocity != 0.0;
        }
    }
    if *stretch != 0.0 || *edge_velocity != 0.0 {
        let side = if *stretch != 0.0 { stretch.signum() } else { edge_velocity.signum() };
        spring(stretch, edge_velocity, 0.0, edge_dt, 484.0, 1.0);
        // A reversal can return to the boundary early. Do not create a new
        // fling away from an edge the user only asked to release.
        if stretch.signum() != side || (stretch.abs() < 0.15 && edge_velocity.abs() < 3.0) {
            *stretch = 0.0; *edge_velocity = 0.0;
        }
    }
    *velocity != 0.0 || *stretch != 0.0 || *edge_velocity != 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spring_is_refresh_rate_independent() {
        let simulate = |hz| { let (mut x, mut v) = (0.35, 3.0); for _ in 0..hz {spring(&mut x, &mut v, 1.0, 1.0/hz as f64, 420.0, 0.85);} (x,v) };
        let reference = simulate(60);
        for hz in [30, 90, 120, 144] { let result = simulate(hz); assert!((result.0-reference.0).abs()<1e-10); assert!((result.1-reference.1).abs()<1e-10); }
    }
    #[test]
    fn resistance_is_progressive_and_reversible() {
        let (mut x, mut stretch) = (0.0, 0.0);
        drag(&mut x, &mut stretch, -100.0, 1000.0);
        let first = stretch;
        drag(&mut x, &mut stretch, -100.0, 1000.0);
        assert!(stretch-first < first);
        drag(&mut x, &mut stretch, 200.0, 1000.0);
        assert!(stretch.abs()<1e-9 && x.abs()<1e-9);
    }
    #[test]
    fn fling_rebounds_at_both_edges_and_settles() {
        for (start, speed) in [(5.0,-1800.0),(995.0,1800.0)] {
            let (mut x,mut v,mut s,mut sv)=(start,speed,0.0,0.0);
            scroll(&mut x,&mut v,&mut s,&mut sv,1000.0,1.0/60.0,4.0,false);
            assert!(s.abs()>0.1,"collision must not stop dead");
            for _ in 0..120 {scroll(&mut x,&mut v,&mut s,&mut sv,1000.0,1.0/60.0,4.0,false);}
            assert_eq!((v,s,sv),(0.0,0.0,0.0));
            assert_eq!(x,if speed<0.0 {0.0}else{1000.0});
        }
    }
    #[test]
    fn grabbing_rebound_keeps_position() {
        let (mut x,mut stretch)=(0.0,32.0);
        drag(&mut x,&mut stretch,0.0,1000.0);
        assert!((stretch-32.0).abs()<1e-9);
        drag(&mut x,&mut stretch,10.0,1000.0);
        assert!(stretch<32.0 && stretch>0.0);
    }
}
