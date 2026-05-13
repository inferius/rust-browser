//! 4x4 transform matrix math + CSS transform-op compose.

use super::TransformOp;

/// 4x4 identity matrix (row-major).
#[inline]
fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// Multiply two 4x4 row-major matrices: out = a * b.
fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0_f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[r * 4 + k] * b[k * 4 + c];
            }
            out[r * 4 + c] = s;
        }
    }
    out
}

/// Vrati matrix pro jeden TransformOp.
fn transform_op_matrix(op: &TransformOp) -> [f32; 16] {
    match op {
        TransformOp::Translate(tx, ty) => [
            1.0, 0.0, 0.0, *tx,
            0.0, 1.0, 0.0, *ty,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
        TransformOp::Translate3D { x, y, z } => [
            1.0, 0.0, 0.0, *x,
            0.0, 1.0, 0.0, *y,
            0.0, 0.0, 1.0, *z,
            0.0, 0.0, 0.0, 1.0,
        ],
        TransformOp::Scale(sx, sy) => [
            *sx, 0.0, 0.0, 0.0,
            0.0, *sy, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
        TransformOp::Scale3D { x, y, z } => [
            *x,  0.0, 0.0, 0.0,
            0.0, *y,  0.0, 0.0,
            0.0, 0.0, *z,  0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
        TransformOp::Rotate(rad) => {
            let c = rad.cos();
            let s = rad.sin();
            [
                c,   -s,  0.0, 0.0,
                s,   c,   0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]
        }
        TransformOp::Rotate3D { x, y, z, angle_rad } => {
            // Rodrigues axis-angle. Predpoklad: osa normalizovana.
            let len = (x*x + y*y + z*z).sqrt();
            let (ux, uy, uz) = if len > 1e-6 {
                (x / len, y / len, z / len)
            } else {
                (0.0, 0.0, 1.0)
            };
            let c = angle_rad.cos();
            let s = angle_rad.sin();
            let one_c = 1.0 - c;
            [
                c + ux*ux*one_c,    ux*uy*one_c - uz*s, ux*uz*one_c + uy*s, 0.0,
                uy*ux*one_c + uz*s, c + uy*uy*one_c,    uy*uz*one_c - ux*s, 0.0,
                uz*ux*one_c - uy*s, uz*uy*one_c + ux*s, c + uz*uz*one_c,    0.0,
                0.0,                0.0,                0.0,                1.0,
            ]
        }
        TransformOp::Matrix3D(m) => *m,
        TransformOp::Perspective(d) => {
            let inv = if d.abs() > 1e-6 { -1.0 / d } else { 0.0 };
            [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, inv, 1.0,
            ]
        }
        TransformOp::None => mat4_identity(),
    }
}

/// Compose vsechny TransformOp do jedne 4x4 matrix.
/// CSS spec: `transform: T1 T2 T3` znamena P' = T1 * T2 * T3 * P
/// (zacina prvni ops zvenku - vlozeny posledni do mat multiplication).
pub fn compute_transform_matrix(ops: &[TransformOp], parent_perspective: Option<f32>) -> [f32; 16] {
    let mut m = mat4_identity();
    // Apply ops in order (left-multiply each)
    for op in ops {
        let opm = transform_op_matrix(op);
        m = mat4_mul(&m, &opm);
    }
    // Parent perspective wraps cely transform: P_persp * T = result
    if let Some(d) = parent_perspective {
        let persp = transform_op_matrix(&TransformOp::Perspective(d));
        m = mat4_mul(&persp, &m);
    }
    m
}

/// True pokud transform vyzaduje 3D pipeline (rotate3d X/Y, perspective,
/// matrix3d s non-zero z, translate3d s nonzero z).
/// Pure 2D transformy (Translate/Scale/Rotate Z) nepotrebuji RT pipeline.
pub fn needs_3d_pipeline(ops: &[TransformOp], parent_perspective: Option<f32>) -> bool {
    // PERF fast-path: 99% elementu nema transform. Bail without iter.
    if ops.is_empty() && parent_perspective.is_none() { return false; }
    if parent_perspective.is_some() {
        // Perspective wrapper trebuje 3D jen pokud transform aspon nejak meni Z
        for op in ops {
            match op {
                TransformOp::Rotate3D { x, y, .. } if x.abs() > 1e-3 || y.abs() > 1e-3 => return true,
                TransformOp::Translate3D { z, .. } if z.abs() > 1e-3 => return true,
                TransformOp::Scale3D { z, .. } if (z - 1.0).abs() > 1e-3 => return true,
                TransformOp::Matrix3D(_) => return true,
                _ => {}
            }
        }
        return false;
    }
    for op in ops {
        match op {
            // 2D rotace -> GPU pipeline (CPU rotate_cmd jen posunul origin,
            // rect zustal axis-aligned).
            TransformOp::Rotate(rad) if rad.abs() > 1e-3 => return true,
            TransformOp::Rotate3D { x, y, .. } if x.abs() > 1e-3 || y.abs() > 1e-3 => return true,
            TransformOp::Perspective(_) => return true,
            TransformOp::Matrix3D(m) => {
                // Detekce 3D matice: m[8]/m[9]/m[2]/m[6]/m[14]/m[11] nenulove
                if m[2].abs() > 1e-3 || m[6].abs() > 1e-3
                    || m[8].abs() > 1e-3 || m[9].abs() > 1e-3
                    || m[11].abs() > 1e-3 || m[14].abs() > 1e-3 {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}
