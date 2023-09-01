use std::borrow::Cow;

use bevy::{
    prelude::{
        App, AssetServer, Commands, FromWorld, Handle, Image, IntoSystemConfigs, Plugin, Res, Resource, Shader, Startup,
    },
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{Node, RenderGraph, self},
        render_resource::{
            AsBindGroup, AsBindGroupError, BindGroup,
            BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
            CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor,
            PipelineCache, ShaderStages,
        },
        renderer::RenderDevice,
        texture::FallbackImage, Render, RenderApp, RenderSet, main_graph::node::CAMERA_DRIVER,
    },
    DefaultPlugins,
};

pub struct NumbersPlugin;

impl Plugin for NumbersPlugin {
    fn build(&self, app: &mut App) {
    }

    fn finish(&self, app: &mut App) {
        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<KernelPipeline>();
            render_app.add_systems(Render, prepare_my_numbers.in_set(RenderSet::Prepare));

            let mut render_graph = render_app.world.get_resource_mut::<RenderGraph>().expect("Should be able to get render graph");
            let kernel_node = DispatchKernel{};

            let id = render_graph.add_node("MY STRING", kernel_node);
            //render_graph.add_node_edge(CAMERA_DRIVER, "MY STRING");
            let r = render_graph.try_add_node_edge(CAMERA_DRIVER, id);
            if r.is_err() {
                println!("{:?}", r);
            }
        }
    }
}

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_plugins(NumbersPlugin)
        .add_plugins(ExtractResourcePlugin::<MyNumbers>::default());

    app.run();
}

fn setup(mut commands: Commands) {
    commands.insert_resource(MyNumbers {
        number: vec![1; 32],
    })
}

#[derive(Resource)]
pub struct KernelPipeline {
    pub pipeline: CachedComputePipelineId,
    pub bind_group_layout: BindGroupLayout,
}

impl FromWorld for KernelPipeline {
    fn from_world(world: &mut bevy::prelude::World) -> Self {
        let render_device = world
            .get_resource::<RenderDevice>()
            .expect("Should be able to get render_device");
        let asset_server = world
            .get_resource::<AssetServer>()
            .expect("Should be able to get asset_server");
        let shader: Handle<Shader> = asset_server.load("kernel.wgsl");

        let entries = vec![BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::COMPUTE,
            ty: bevy::render::render_resource::BindingType::Buffer {
                ty: bevy::render::render_resource::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];

        let bind_group_layout =
            render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("Kernel Pipeline Bind Group Layout"),
                entries: entries.as_slice(),
            });
        let pipeline_cache = world.resource_mut::<PipelineCache>();
        let kernel_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(Cow::from("kernel pipeline")),
            layout: vec![bind_group_layout.clone()],
            push_constant_ranges: vec![],
            shader,
            shader_defs: vec![],
            entry_point: Cow::from("main"),
        });

        KernelPipeline {
            pipeline: kernel_pipeline,
            bind_group_layout,
        }
    }
}

#[derive(Resource, AsBindGroup, Debug, Clone, ExtractResource)]
pub struct MyNumbers {
    #[storage(0, visibility(compute))]
    number: Vec<i32>,
}

#[derive(Debug, Resource)]
pub struct KernelBindGroup(pub BindGroup);

fn prepare_my_numbers(
    mut commands: Commands,
    numbers: Res<MyNumbers>,
    render_device: Res<RenderDevice>,
    pipeline: Res<KernelPipeline>,
    fallback_image: Res<FallbackImage>,
    images: Res<RenderAssets<Image>>,
) {
    let prepared_result = numbers.as_bind_group(
        &pipeline.bind_group_layout,
        &render_device,
        &images,
        &fallback_image,
    );
    match prepared_result {
        Ok(prepared_numbers) => {
            commands.insert_resource(KernelBindGroup(prepared_numbers.bind_group));
        }
        Err(AsBindGroupError::RetryNextUpdate) => {
            println!("retry next update");
            // we are retrying every frame regardless
        }
    }
}
pub struct DispatchKernel;

impl Node for DispatchKernel {
    fn run(
        &self,
        graph: &mut bevy::render::render_graph::RenderGraphContext,
        render_context: &mut bevy::render::renderer::RenderContext,
        world: &bevy::prelude::World,
    ) -> Result<(), bevy::render::render_graph::NodeRunError> {
        // can't use because there is no view entity, uncommenting this line causes a hard-to-diagnose panic
        //let _view_entity = graph.view_entity();
        let kernel_pipeline = world.get_resource::<KernelPipeline>();
        let kernel_bind_group = world.get_resource::<KernelBindGroup>();
        let pipeline_cache = world.get_resource::<PipelineCache>();
        if let (Some(kernel_pipeline), Some(kernel_bind_group), Some(pipeline_cache)) = (kernel_pipeline, kernel_bind_group, pipeline_cache)
        {
            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("Kernel Compute Pass"),
                    });
            if let Some(real_pipeline) =
                pipeline_cache.get_compute_pipeline(kernel_pipeline.pipeline)
            {
                println!("dispatch happening");
                pass.set_pipeline(&real_pipeline);
                pass.set_bind_group(0, &kernel_bind_group.0, &[]);
                pass.dispatch_workgroups(1, 1, 1);
            }
        }
        Ok(())
    }
}
