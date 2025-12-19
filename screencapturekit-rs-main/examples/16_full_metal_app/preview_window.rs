//! Metal preview window for captured frames

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::mem::size_of;

use cocoa::appkit::NSView;
use cocoa::base::id as cocoa_id;
use core_graphics_types::geometry::CGSize;
use metal::{
    Device, MTLClearColor, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType,
    MTLResourceOptions, MTLStoreAction, MetalLayer, RenderPassDescriptor,
};
use objc::rc::autoreleasepool;
use objc::runtime::YES;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

use crate::capture::CaptureState;
use crate::renderer::{create_textures_from_iosurface, CaptureTextures, 
    PIXEL_FORMAT_420F, PIXEL_FORMAT_420V, SHADER_SOURCE};
use crate::vertex::Uniforms;

pub fn run_preview_window(
    capture_state: Arc<CaptureState>,
    is_capturing: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let event_loop = EventLoop::new();
        let window = winit::window::WindowBuilder::new()
            .with_inner_size(winit::dpi::LogicalSize::new(960, 540))
            .with_title("Screen Capture Preview")
            .build(&event_loop)
            .unwrap();

        // Initialize Metal
        let device = Device::system_default().expect("No Metal device found");

        let mut layer = MetalLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        layer.set_presents_with_transaction(false);

        // Attach layer to window
        unsafe {
            match window.raw_window_handle() {
                RawWindowHandle::AppKit(handle) => {
                    let view = handle.ns_view as cocoa_id;
                    view.setWantsLayer(YES);
                    view.setLayer(std::mem::transmute(layer.as_mut()));
                }
                _ => panic!("Unsupported window handle"),
            }
        }

        let draw_size = window.inner_size();
        layer.set_drawable_size(CGSize::new(
            f64::from(draw_size.width),
            f64::from(draw_size.height),
        ));

        // Compile shaders
        let compile_options = metal::CompileOptions::new();
        let library = device
            .new_library_with_source(SHADER_SOURCE, &compile_options)
            .expect("Failed to compile shaders");

        // Create fullscreen textured pipeline
        let fullscreen_pipeline = {
            let vert = library.get_function("vertex_fullscreen", None).unwrap();
            let frag = library.get_function("fragment_textured", None).unwrap();
            let desc = metal::RenderPipelineDescriptor::new();
            desc.set_vertex_function(Some(&vert));
            desc.set_fragment_function(Some(&frag));
            desc.color_attachments()
                .object_at(0)
                .unwrap()
                .set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            device.new_render_pipeline_state(&desc).unwrap()
        };

        // Create YCbCr pipeline
        let ycbcr_pipeline = {
            let vert = library.get_function("vertex_fullscreen", None).unwrap();
            let frag = library.get_function("fragment_ycbcr", None).unwrap();
            let desc = metal::RenderPipelineDescriptor::new();
            desc.set_vertex_function(Some(&vert));
            desc.set_fragment_function(Some(&frag));
            desc.color_attachments()
                .object_at(0)
                .unwrap()
                .set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            device.new_render_pipeline_state(&desc).unwrap()
        };

        let command_queue = device.new_command_queue();
        let mut time = 0.0f32;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = ControlFlow::Exit;
                }
                Event::MainEventsCleared => {
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    autoreleasepool(|| {
                        time += 0.016;

                        let size = window.inner_size();
                        let width = size.width as f32;
                        let height = size.height as f32;

                        // Try to get captured frame
                        let mut capture_textures: Option<CaptureTextures> = None;
                        let mut tex_width = 1280.0f32;
                        let mut tex_height = 720.0f32;
                        let mut pixel_format: u32 = 0;

                        if is_capturing.load(Ordering::Relaxed) {
                            if let Ok(guard) = capture_state.latest_surface.try_lock() {
                                if let Some(ref surface) = *guard {
                                    tex_width = surface.width() as f32;
                                    tex_height = surface.height() as f32;
                                    capture_textures = unsafe {
                                        create_textures_from_iosurface(&device, surface.as_ptr())
                                    };
                                    if let Some(ref ct) = capture_textures {
                                        pixel_format = ct.pixel_format;
                                    }
                                }
                            }
                        }

                        // Uniforms
                        let uniforms = Uniforms {
                            viewport_size: [width, height],
                            texture_size: [tex_width, tex_height],
                            time,
                            pixel_format,
                            _padding: [0.0; 2],
                        };
                        let uniforms_buffer = device.new_buffer_with_data(
                            std::ptr::addr_of!(uniforms).cast(),
                            size_of::<Uniforms>() as u64,
                            MTLResourceOptions::CPUCacheModeDefaultCache,
                        );

                        // Render
                        let Some(drawable) = layer.next_drawable() else {
                            return;
                        };

                        let render_pass = RenderPassDescriptor::new();
                        let attachment = render_pass.color_attachments().object_at(0).unwrap();
                        attachment.set_texture(Some(drawable.texture()));
                        attachment.set_load_action(MTLLoadAction::Clear);
                        attachment.set_clear_color(MTLClearColor::new(0.08, 0.08, 0.1, 1.0));
                        attachment.set_store_action(MTLStoreAction::Store);

                        let cmd_buffer = command_queue.new_command_buffer();
                        let encoder = cmd_buffer.new_render_command_encoder(render_pass);

                        // Draw captured frame if available
                        if let Some(ref textures) = capture_textures {
                            let is_ycbcr = textures.pixel_format == PIXEL_FORMAT_420V
                                || textures.pixel_format == PIXEL_FORMAT_420F;

                            if is_ycbcr && textures.plane1.is_some() {
                                encoder.set_render_pipeline_state(&ycbcr_pipeline);
                                encoder.set_vertex_buffer(0, Some(&uniforms_buffer), 0);
                                encoder.set_fragment_texture(0, Some(&textures.plane0));
                                encoder.set_fragment_texture(1, Some(textures.plane1.as_ref().unwrap()));
                                encoder.set_fragment_buffer(0, Some(&uniforms_buffer), 0);
                            } else {
                                encoder.set_render_pipeline_state(&fullscreen_pipeline);
                                encoder.set_vertex_buffer(0, Some(&uniforms_buffer), 0);
                                encoder.set_fragment_texture(0, Some(&textures.plane0));
                            }
                            encoder.draw_primitives(MTLPrimitiveType::TriangleStrip, 0, 4);
                        }

                        encoder.end_encoding();
                        cmd_buffer.present_drawable(drawable);
                        cmd_buffer.commit();
                    });
                }
                _ => {}
            }
        });
    });
}

