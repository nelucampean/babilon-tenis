//! Spark particles (bounce hits, point celebrations) + their assets.
use bevy::prelude::*;
use bevy::render::mesh::Mesh2d;
use bevy::sprite::MeshMaterial2d;
use rand::Rng;

/// Spark particles. Mirrors `particles` + `spark()` in index.html.
pub(crate) struct Fx {
    entity: Entity,
    vel: Vec2,
    life: f32,
    max: f32,
}

#[derive(Resource, Default)]
pub(crate) struct Particles {
    items: Vec<Fx>,
}

#[derive(Resource)]
pub(crate) struct ParticleAssets {
    pub(crate) mesh: Handle<Mesh>,
    pub(crate) gold: Handle<ColorMaterial>,
    pub(crate) pale: Handle<ColorMaterial>,
}

pub(crate) fn spawn_sparks(
    commands: &mut Commands,
    assets: &ParticleAssets,
    particles: &mut Particles,
    pos: Vec2,
    gold: bool,
    n: usize,
) {
    let mut rng = rand::thread_rng();
    for _ in 0..n {
        let a: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
        let sp = rng.gen_range(60.0..220.0);
        let e = commands
            .spawn((
                Mesh2d(assets.mesh.clone()),
                MeshMaterial2d(if gold {
                    assets.gold.clone()
                } else {
                    assets.pale.clone()
                }),
                Transform::from_xyz(pos.x, pos.y, 5.0),
            ))
            .id();
        particles.items.push(Fx {
            entity: e,
            vel: Vec2::new(a.cos() * sp, a.sin() * sp + 60.0),
            life: 0.42,
            max: 0.42,
        });
    }
}

/// Spark particles: rise, fall with gravity, shrink and despawn.
pub(crate) fn particle_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: ResMut<Particles>,
    mut q: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    particles.items.retain_mut(|p| {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(p.entity).despawn();
            return false;
        }
        p.vel.y -= 520.0 * dt;
        if let Ok(mut tr) = q.get_mut(p.entity) {
            tr.translation.x += p.vel.x * dt;
            tr.translation.y += p.vel.y * dt;
            let k = (p.life / p.max).clamp(0.0, 1.0);
            tr.scale = Vec3::splat(k.max(0.15));
        }
        true
    });
}
