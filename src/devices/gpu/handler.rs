use super::{
    Gpu,
    cmd::{Gp0Opcode, Gp1Opcode},
};

pub fn handle_gp0(gpu: &mut Gpu, value: u32) {
    let opcode = (value >> 24) as u8;
    let Some(opcode) = Gp0Opcode::from_repr(opcode) else {
        return;
    };

    match opcode {
        Gp0Opcode::Nop => {}
        Gp0Opcode::ClearTextureCache => todo!(),
        Gp0Opcode::FillRectangleInVram => todo!(),
        Gp0Opcode::InterruptRequest => todo!(),
        Gp0Opcode::Monochrome3PointPolygonOpaque => todo!(),
        Gp0Opcode::Monochrome3PointPolygonSemiTransparent => todo!(),
        Gp0Opcode::Textured3PointPolygonOpaqueBlend => todo!(),
        Gp0Opcode::Textured3PointPolygonOpaqueRaw => todo!(),
        Gp0Opcode::Textured3PointPolygonSemiTransparentBlend => todo!(),
        Gp0Opcode::Textured3PointPolygonSemiTransparentRaw => todo!(),
        Gp0Opcode::Monochrome4PointPolygonOpaque => todo!(),
        Gp0Opcode::Monochrome4PointPolygonSemiTransparent => todo!(),
        Gp0Opcode::Textured4PointPolygonOpaqueBlend => todo!(),
        Gp0Opcode::Textured4PointPolygonOpaqueRaw => todo!(),
        Gp0Opcode::Textured4PointPolygonSemiTransparentBlend => todo!(),
        Gp0Opcode::Textured4PointPolygonSemiTransparentRaw => todo!(),
        Gp0Opcode::Shaded3PointPolygonOpaque => todo!(),
        Gp0Opcode::Shaded3PointPolygonSemiTransparent => todo!(),
        Gp0Opcode::ShadedTextured3PointPolygonOpaqueBlend => todo!(),
        Gp0Opcode::ShadedTextured3PointPolygonOpaqueRaw => todo!(),
        Gp0Opcode::ShadedTextured3PointPolygonSemiTransparentBlend => todo!(),
        Gp0Opcode::ShadedTextured3PointPolygonSemiTransparentRaw => todo!(),
        Gp0Opcode::Shaded4PointPolygonOpaque => todo!(),
        Gp0Opcode::Shaded4PointPolygonSemiTransparent => todo!(),
        Gp0Opcode::ShadedTextured4PointPolygonOpaqueBlend => todo!(),
        Gp0Opcode::ShadedTextured4PointPolygonOpaqueRaw => todo!(),
        Gp0Opcode::ShadedTextured4PointPolygonSemiTransparentBlend => todo!(),
        Gp0Opcode::ShadedTextured4PointPolygonSemiTransparentRaw => todo!(),
        Gp0Opcode::MonochromeLineOpaque => todo!(),
        Gp0Opcode::MonochromeLineSemiTransparent => todo!(),
        Gp0Opcode::ShadedLineOpaque => todo!(),
        Gp0Opcode::ShadedLineSemiTransparent => todo!(),
        Gp0Opcode::MonochromePolylineOpaque => todo!(),
        Gp0Opcode::MonochromePolylineSemiTransparent => todo!(),
        Gp0Opcode::ShadedPolylineOpaque => todo!(),
        Gp0Opcode::ShadedPolylineSemiTransparent => todo!(),
        Gp0Opcode::MonochromeRectangleVariableOpaque => todo!(),
        Gp0Opcode::MonochromeRectangleVariableSemiTransparent => todo!(),
        Gp0Opcode::TexturedRectangleVariableOpaqueBlend => todo!(),
        Gp0Opcode::TexturedRectangleVariableOpaqueRaw => todo!(),
        Gp0Opcode::TexturedRectangleVariableSemiTransparentBlend => todo!(),
        Gp0Opcode::TexturedRectangleVariableSemiTransparentRaw => todo!(),
        Gp0Opcode::DotRectangleOpaque => todo!(),
        Gp0Opcode::DotRectangleSemiTransparent => todo!(),
        Gp0Opcode::Sprite8x8OpaqueBlend => todo!(),
        Gp0Opcode::Sprite8x8OpaqueRaw => todo!(),
        Gp0Opcode::Sprite8x8SemiTransparentBlend => todo!(),
        Gp0Opcode::Sprite8x8SemiTransparentRaw => todo!(),
        Gp0Opcode::Sprite16x16OpaqueBlend => todo!(),
        Gp0Opcode::Sprite16x16OpaqueRaw => todo!(),
        Gp0Opcode::Sprite16x16SemiTransparentBlend => todo!(),
        Gp0Opcode::Sprite16x16SemiTransparentRaw => todo!(),
        Gp0Opcode::CopyRectangleVramToVram => todo!(),
        Gp0Opcode::CopyRectangleCpuToVram => todo!(),
        Gp0Opcode::CopyRectangleVramToCpu => todo!(),
        Gp0Opcode::DrawMode => todo!(),
        Gp0Opcode::TextureWindow => todo!(),
        Gp0Opcode::DrawingAreaTopLeft => todo!(),
        Gp0Opcode::DrawingAreaBottomRight => todo!(),
        Gp0Opcode::DrawingOffset => todo!(),
        Gp0Opcode::MaskBitSetting => todo!(),
    }
}

pub fn handle_gp1(gpu: &mut Gpu, value: u32) {
    let opcode = (value >> 24) as u8;
    let Some(opcode) = Gp1Opcode::from_repr(opcode) else {
        return;
    };

    match opcode {
        Gp1Opcode::ResetGpu => todo!(),
        Gp1Opcode::ResetCommandBuffer => todo!(),
        Gp1Opcode::AcknowledgeInterrupt => todo!(),
        Gp1Opcode::DisplayEnable => todo!(),
        Gp1Opcode::DmaDirection => todo!(),
        Gp1Opcode::DisplayVramStart => todo!(),
        Gp1Opcode::DisplayHorizontalRange => todo!(),
        Gp1Opcode::DisplayVerticalRange => todo!(),
        Gp1Opcode::DisplayMode => todo!(),
        Gp1Opcode::GetGpuInfo => todo!(),
    }
}
