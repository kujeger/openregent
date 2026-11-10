// A tree bending in the wind: its quad sheared about its foot, so each
// pixel keeps its row. `regent_draw::wind` holds the numbers and the why;
// `material::Plugin` loads this, `body.wesl` and `band.wesl` import it.

const TAU: f32 = 6.2831855;

// `wind::Gust`.
struct Gust {
    at: f32,
    reach: f32,
    foot: f32,
    bend: f32,
    period: f32,
    wave: f32,
    gust: f32,
    gust_wave: f32,
};

/// Local point `local` of a quad placed by `world_from_local`, in the
/// world, leaning east by the wind at the tree's x: 0 at the foot, the
/// most at the top.
fn swayed(world_from_local: mat4x4<f32>, local: vec3<f32>, wind: Gust) -> vec4<f32> {
    let world = world_from_local * vec4<f32>(local, 1.0);
    let x = world_from_local[3].x;

    // Never negative: east and back.
    let sway = 0.5 + 0.5 * sin(TAU * (wind.at / wind.period - x / wind.wave));
    let gust = 0.75 + 0.25 * sin(TAU * (wind.at / wind.gust - x / wind.gust_wave));
    let lean = wind.reach * sway * gust;

    // The foot stays; the top moves `lean` · factor · height / reference
    // (`wind::Trees::bend`), as `wind::Gust::shove`.
    let share = (local.y - wind.foot) * wind.bend;

    return world + vec4<f32>(lean * share, 0.0, 0.0, 0.0);
}
