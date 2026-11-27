//! Building shaders off the render thread. naga plus Apple's shader compiler can take a good fraction of a second
//! per shader, so nothing here is built while a frame is drawn:
//!
//! - imported shaders (ISF) are built on a worker the first time they're drawn (blank until then), a few at a time;
//! - one whose file changed is built again on a worker while the old one keeps drawing, and swapped in when ready;
//! - Shader Editor / patch compiles (`compile_custom_async`) are checked and built on a worker; the shader on
//!   screen keeps drawing its last good version until the new one is ready (or the error comes back);
//! - pipelines for the other picture formats a shader may draw into (deep colour's 16-bit, points' float buffers)
//!   are made with it, and the built-in shaders' are made on a worker the first frame those formats are used.
//!
//! A compile's names and description are interned (`shaders::intern`, `intern_desc`): compiling again keeps nothing
//! more (every compile used to leak a new entry name and description).

use super::*;
use crate::custom::{CompileError, Header};
use crate::shaders::{self, ShaderDesc};

/// Shaders built at the same time on worker threads (Apple's shader compiler is heavy; more would stutter the show).
const MAX_COMPILING: usize = 3;

/// A module and its pipelines, made on a worker.
pub(super) struct Built {
    module: wgpu::ShaderModule,
    pipelines: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
}

/// What a worker sends back.
enum Finished {
    /// An imported shader's build for `generation` (see `Builds::generations`).
    Imported { entry: &'static str, generation: u64, result: Result<Built, String> },
    /// A user shader's compile (`ticket` = which request it answers).
    Custom { id: String, ticket: u64, result: Result<Built, CompileError> },
    /// One of the pipelines made ahead for another picture format, with the module it was made from.
    Pipeline { entry: &'static str, format: wgpu::TextureFormat, module: wgpu::ShaderModule, pipeline: wgpu::RenderPipeline },
}

/// What an imported shader brings besides its code. Applied when its build is drawn: for a file that changed, only
/// once the new build replaces the old (the old code with the new pass plan would draw wrong).
#[derive(Default)]
pub struct ShaderSetup {
    pub plan: Option<shaders::PassPlan>,
    pub audio_slots: Vec<(usize, shaders::AudioTexture)>,
    pub vertex_entry: Option<&'static str>,
    pub instances: Option<shaders::Instances>,
    /// Pictures it imports: (buffer slot, width, height, RGBA8).
    pub images: Vec<(usize, u32, u32, Vec<u8>)>,
}

/// An imported shader being built again (its file changed), and what goes with the new build.
struct Replacement {
    desc: &'static ShaderDesc,
    setup: ShaderSetup,
}

/// A user shader compile asked for.
struct CustomJob {
    ticket: u64,
    entry: &'static str,
    header: Header,
    /// The source until a worker takes it.
    waiting: Option<String>,
}

/// A user shader compile's answer (`Engine::take_custom_compiled`).
pub struct CustomCompiled {
    pub id: String,
    pub result: Result<&'static ShaderDesc, CompileError>,
}

pub(super) struct Builds {
    /// Imported shaders waiting to be built on first use: entry → WGSL (without prelude).
    pending: HashMap<&'static str, String>,
    /// First builds on a worker right now (drawn blank until they arrive).
    compiling: HashSet<&'static str>,
    /// The build each imported shader waits for: a result for an older one (a file saved twice quickly) is dropped.
    generations: HashMap<&'static str, u64>,
    replacing: HashMap<&'static str, Replacement>,
    custom: HashMap<String, CustomJob>,
    /// User shader compiles on a worker now.
    custom_running: usize,
    next_ticket: u64,
    done: Vec<CustomCompiled>,
    /// Picture formats whose built-in pipelines were (or are being) made ahead.
    prewarmed: HashSet<wgpu::TextureFormat>,
    /// What went wrong rebuilding a changed file (the old version stayed), for the app's log (`take_build_notes`).
    notes: Vec<String>,
    tx: std::sync::mpsc::Sender<Finished>,
    rx: std::sync::mpsc::Receiver<Finished>,
}

impl Default for Builds {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            pending: HashMap::new(),
            compiling: HashSet::new(),
            generations: HashMap::new(),
            replacing: HashMap::new(),
            custom: HashMap::new(),
            custom_running: 0,
            next_ticket: 0,
            done: Vec::new(),
            prewarmed: HashSet::new(),
            notes: Vec::new(),
            tx,
            rx,
        }
    }
}

impl Builds {
    fn running(&self) -> usize {
        self.compiling.len() + self.custom_running
    }
}

/// Makes `entry`'s module from `full` (prelude included) and its pipelines for `formats`, on a worker thread.
/// `vertex` / `instanced`: the vertex stage and blending it's drawn with (a changed file's, before it's applied).
fn build(device: &wgpu::Device, layout: &wgpu::PipelineLayout, entry: &'static str, full: String, prelude_lines: u32, formats: &[wgpu::TextureFormat], vertex: Option<&'static str>, instanced: bool) -> Result<Built, CompileError> {
    crate::custom::validate(&full, prelude_lines)?;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(entry), source: wgpu::ShaderSource::Wgsl(full.into()) });
    let pipelines = formats.iter().map(|&f| (f, crate::gpu::build_pipeline_as(device, layout, &module, entry, f, vertex, instanced))).collect();
    Ok(Built { module, pipelines })
}

fn spawn(name: String, job: impl FnOnce() + Send + 'static) -> bool {
    std::thread::Builder::new().name(name).spawn(job).is_ok()
}

impl Engine {
    /// The picture formats a shader is built for: the 8-bit mix, deep colour's when the show uses it, and float
    /// when its passes keep float buffers (made with it, so its first draw in them doesn't build them mid-frame).
    fn formats_for(&self, plan: Option<&shaders::PassPlan>) -> Vec<wgpu::TextureFormat> {
        let mut formats = vec![crate::gpu::FORMAT];
        if self.mix_format != crate::gpu::FORMAT || self.builds.prewarmed.contains(&crate::gpu::DEEP_FORMAT) {
            formats.push(crate::gpu::DEEP_FORMAT);
        }
        if plan.is_some_and(|p| p.passes.iter().any(|pp| pp.float)) {
            formats.push(wgpu::TextureFormat::Rgba16Float);
        }
        formats.dedup();
        formats
    }

    /// Registers an already-converted shader (e.g. ISF) that is compiled the first time it is
    /// drawn. `wgsl` must define `@fragment fn <entry>(in: VOut)` on top of the common prelude.
    pub fn register_lazy(&mut self, desc: &'static ShaderDesc, wgsl: String, setup: ShaderSetup) {
        // Already built (a rescan of the library): keep it, so what's on screen doesn't blink while it rebuilds.
        if !self.gpu.custom_modules.contains_key(desc.entry) && !self.builds.compiling.contains(desc.entry) {
            self.builds.pending.insert(desc.entry, wgsl);
        }
        self.apply_setup(desc.entry, setup);
        shaders::register_custom(desc);
    }

    /// An imported shader whose file changed. Built again on a worker while the old one keeps drawing, then swapped
    /// in with its new settings (it went blank until rebuilt: a file touched by a cloud sync blanked a layer mid-show).
    /// One that wasn't built yet just waits for its first use with the new code.
    pub fn replace_lazy(&mut self, desc: &'static ShaderDesc, wgsl: String, setup: ShaderSetup) {
        let entry = desc.entry;
        if !self.gpu.custom_modules.contains_key(entry) && !self.builds.compiling.contains(entry) {
            self.failed_modules.remove(entry);
            self.builds.pending.insert(entry, wgsl);
            self.builds.replacing.remove(entry);
            self.apply_setup(entry, setup);
            shaders::register_custom(desc);
            return;
        }
        let generation = {
            let g = self.builds.generations.entry(entry).or_default();
            *g += 1;
            *g
        };
        let (device, layout) = self.gpu.compile_parts();
        let formats = self.formats_for(setup.plan.as_ref());
        let (vertex, instanced) = (setup.vertex_entry, setup.instances.is_some());
        let tx = self.builds.tx.clone();
        let started = spawn(format!("shader: {entry}"), move || {
            let full = format!("{}\n{wgsl}", shaders::common_source());
            let prelude_lines = shaders::common_source().lines().count() as u32 + 1;
            let result = build(&device, &layout, entry, full, prelude_lines, &formats, vertex, instanced).map_err(|e| e.to_string());
            let _ = tx.send(Finished::Imported { entry, generation, result });
        });
        if started {
            self.builds.replacing.insert(entry, Replacement { desc, setup });
        } else {
            self.builds.notes.push(format!("{}: changed on disk, but the shader compiler thread didn't start: the old version stays", desc.name));
        }
    }

    /// A shader's settings besides its code, in place now (`ShaderSetup`).
    fn apply_setup(&mut self, entry: &'static str, setup: ShaderSetup) {
        shaders::forget_entry(entry);
        self.plan_images.retain(|(e, _), _| *e != entry);
        if let Some(plan) = setup.plan {
            shaders::set_pass_plan(entry, plan);
        }
        shaders::set_audio_slots(entry, setup.audio_slots);
        if let Some(v) = setup.vertex_entry {
            shaders::set_vertex_entry(entry, v);
        }
        if let Some(n) = setup.instances {
            shaders::set_instances(entry, n);
        }
        for (slot, w, h, rgba) in setup.images {
            self.set_plan_image(entry, slot, w, h, &rgba);
        }
    }

    /// Problems rebuilding changed shader files since the last call (the old versions kept drawing): for the log.
    pub fn take_build_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.builds.notes)
    }

    /// Is an imported shader still being built (a first build draws blank until then; a changed file's old version
    /// keeps drawing)? Failures are in `failed_modules`.
    pub fn shader_pending(&self, entry: &str) -> bool {
        self.builds.pending.contains_key(entry) || self.builds.compiling.contains(entry) || self.builds.replacing.contains_key(entry)
    }

    /// Is this shader ready to draw? An imported shader used for the first time is validated and built on a
    /// worker thread, so the show never hitches: the layer draws blank until it arrives. A few build at a time
    /// (opening a big effects list asks for dozens at once): the rest wait their turn.
    pub(super) fn ensure_module(&mut self, entry: &'static str) -> bool {
        if self.failed_modules.contains_key(entry) || self.builds.compiling.contains(entry) {
            return false;
        }
        if !self.builds.pending.contains_key(entry) {
            // (A changed file whose first build was dropped for the newer one: nothing to draw until that's in.)
            return !self.builds.replacing.contains_key(entry) || self.gpu.custom_modules.contains_key(entry);
        }
        if self.builds.running() >= MAX_COMPILING {
            return false;
        }
        let Some(src) = self.builds.pending.remove(entry) else { return true };
        let generation = *self.builds.generations.entry(entry).or_default();
        let (device, layout) = self.gpu.compile_parts();
        let formats = self.formats_for(shaders::pass_plan(entry).as_deref());
        let tx = self.builds.tx.clone();
        let started = spawn(format!("shader: {entry}"), move || {
            let full = format!("{}\n{src}", shaders::common_source());
            let prelude_lines = shaders::common_source().lines().count() as u32 + 1;
            let result = build(&device, &layout, entry, full, prelude_lines, &formats, shaders::vertex_entry(entry), shaders::instances(entry).is_some()).map_err(|e| e.to_string());
            let _ = tx.send(Finished::Imported { entry, generation, result });
        });
        if started {
            self.builds.compiling.insert(entry);
        } else {
            self.failed_modules.insert(entry, "couldn't start the shader compiler thread".into());
        }
        false
    }

    /// Takes in what the workers finished since the last frame (called before a frame is drawn), and starts user
    /// shader compiles waiting for a free worker.
    pub(super) fn collect_compiled(&mut self) {
        while let Ok(finished) = self.builds.rx.try_recv() {
            match finished {
                Finished::Imported { entry, generation, result } => {
                    self.builds.compiling.remove(entry);
                    // An older build than the one waited for: the newer one is coming.
                    if self.builds.generations.get(entry).copied().unwrap_or(0) != generation {
                        continue;
                    }
                    let replacement = self.builds.replacing.remove(entry);
                    match (result, replacement) {
                        (Ok(built), Some(r)) => {
                            self.apply_setup(entry, r.setup);
                            shaders::register_custom(r.desc);
                            self.failed_modules.remove(entry);
                            self.install(entry, built);
                        }
                        (Ok(built), None) => self.install(entry, built),
                        (Err(e), Some(r)) => self.builds.notes.push(format!("{}: changed on disk but doesn't build, the old version stays: {e}", r.desc.name)),
                        (Err(e), None) => {
                            self.failed_modules.insert(entry, e);
                        }
                    }
                }
                Finished::Custom { id, ticket, result } => {
                    self.builds.custom_running = self.builds.custom_running.saturating_sub(1);
                    // A newer compile of it was asked for (or it was deleted): this one is out of date.
                    if self.builds.custom.get(&id).is_none_or(|j| j.ticket != ticket) {
                        continue;
                    }
                    let Some(job) = self.builds.custom.remove(&id) else { continue };
                    let result = result.map(|built| self.finish_custom(&id, &job.header, job.entry, built));
                    self.builds.done.push(CustomCompiled { id, result });
                }
                Finished::Pipeline { entry, format, module, pipeline } => {
                    // Only while that module is still the one drawn (a shader compiled again meanwhile: dropped).
                    let current = *self.gpu.custom_modules.get(entry).unwrap_or(self.gpu.builtin_module()) == module;
                    if current {
                        self.gpu.add_pipeline(entry, format, pipeline);
                    }
                }
            }
        }
        self.start_custom_compiles();
    }

    /// A module and its pipelines in place of whatever `entry` had.
    fn install(&mut self, entry: &'static str, built: Built) {
        self.gpu.forget_custom(entry);
        self.gpu.install_custom(entry, built.module, built.pipelines);
    }

    /// The entry a user shader `id` compiles under: never the one drawing now, so that one keeps drawing until the
    /// new build is ready. Two names in turn (interned: compiling again makes no new one).
    fn custom_entry(&self, id: &str) -> &'static str {
        if let Some(job) = self.builds.custom.get(id) {
            return job.entry;
        }
        let slug: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let a = shaders::intern(&format!("custom_{slug}_a"));
        if shaders::custom_desc(id).is_some_and(|d| d.entry == a) { shaders::intern(&format!("custom_{slug}_b")) } else { a }
    }

    /// Compiles a user shader (see `custom`) now, on this thread. On success it is registered under `id` and can
    /// be used like any built-in effect or generator. For the app's own edits use `compile_custom_async`.
    pub fn compile_custom(&mut self, id: &str, source: &str) -> Result<&'static ShaderDesc, CompileError> {
        let header = crate::custom::parse_header(source)?;
        // (A compile on a worker for it is out of date now.)
        self.builds.custom.remove(id);
        let entry = self.custom_entry(id);
        let (full, prelude_lines) = crate::custom::module_source(source, entry)?;
        let formats = self.formats_for(crate::custom::plan_for(&header).as_ref());
        let built = build(&self.gpu.device, &self.gpu.compile_parts().1, entry, full, prelude_lines, &formats, None, false)?;
        Ok(self.finish_custom(id, &header, entry, built))
    }

    /// Compiles a user shader on a worker thread: the Shader Editor and patches (each pause in typing), and the
    /// saved shaders at start. What's on screen keeps its last good build until this one is ready. The answer comes
    /// from `take_custom_compiled` (only the newest one asked for: older ones still running are dropped). A header
    /// error (a bad `// @param`) comes back right away.
    pub fn compile_custom_async(&mut self, id: &str, source: &str) -> Result<(), CompileError> {
        let header = crate::custom::parse_header(source)?;
        let entry = self.custom_entry(id);
        self.builds.next_ticket += 1;
        let ticket = self.builds.next_ticket;
        self.builds.custom.insert(id.to_string(), CustomJob { ticket, entry, header, waiting: Some(source.to_string()) });
        self.start_custom_compiles();
        Ok(())
    }

    /// Forget a compile of user shader `id` still on its way (its answer is dropped); what's registered stays.
    pub fn cancel_custom_compile(&mut self, id: &str) {
        self.builds.custom.remove(id);
    }

    /// Is a compile of user shader `id` asked for and not answered yet?
    pub fn custom_compiling(&self, id: &str) -> bool {
        self.builds.custom.contains_key(id)
    }

    /// User shader compiles answered since the last call (see `compile_custom_async`).
    pub fn take_custom_compiled(&mut self) -> Vec<CustomCompiled> {
        std::mem::take(&mut self.builds.done)
    }

    /// Hands waiting user shader compiles to workers while there's room.
    fn start_custom_compiles(&mut self) {
        while self.builds.running() < MAX_COMPILING {
            let Some((id, job)) = self.builds.custom.iter_mut().find(|(_, j)| j.waiting.is_some()) else { return };
            let (id, source, entry, ticket, plan) = (id.clone(), job.waiting.take().unwrap_or_default(), job.entry, job.ticket, crate::custom::plan_for(&job.header));
            let formats = self.formats_for(plan.as_ref());
            let (device, layout) = self.gpu.compile_parts();
            let tx = self.builds.tx.clone();
            let worker_id = id.clone();
            let job = move || {
                let result = crate::custom::module_source(&source, entry).and_then(|(full, prelude_lines)| build(&device, &layout, entry, full, prelude_lines, &formats, None, false));
                let _ = tx.send(Finished::Custom { id: worker_id, ticket, result });
            };
            // (No threads on the web: built here, as before.)
            #[cfg(target_arch = "wasm32")]
            let started = {
                job();
                true
            };
            #[cfg(not(target_arch = "wasm32"))]
            let started = spawn(format!("shader: {entry}"), job);
            if started {
                self.builds.custom_running += 1;
            } else {
                self.builds.custom.remove(&id);
                self.builds.done.push(CustomCompiled { id, result: Err(CompileError { line: None, message: "couldn't start the shader compiler thread".into() }) });
            }
        }
    }

    /// A user shader's new build in place: registered under `id` (its pass plan with it), and the build it replaces
    /// dropped (its module, pipelines and pass plan; every compile used to leave them behind).
    fn finish_custom(&mut self, id: &str, header: &Header, entry: &'static str, built: Built) -> &'static ShaderDesc {
        shaders::forget_entry(entry);
        self.install(entry, built);
        if let Some(plan) = crate::custom::plan_for(header) {
            shaders::set_pass_plan(entry, plan);
        }
        let desc = crate::custom::make_desc(id, header, entry);
        let old = shaders::custom_desc(id).map(|d| d.entry).filter(|e| *e != entry && e.starts_with("custom_"));
        shaders::register_custom(desc);
        if let Some(old) = old {
            self.forget_custom_entry(old);
        }
        desc
    }

    /// A user shader deleted (or renamed): unlisted, its compiled module and pipelines dropped, and a compile of it
    /// still running ignored.
    pub fn remove_custom(&mut self, id: &str) {
        self.builds.custom.remove(id);
        let old = shaders::custom_desc(id).map(|d| d.entry).filter(|e| e.starts_with("custom_"));
        shaders::unregister_custom(id);
        if let Some(old) = old {
            self.forget_custom_entry(old);
        }
    }

    /// Drops everything kept for a user shader's entry that nothing draws any more (an older compile).
    fn forget_custom_entry(&mut self, entry: &'static str) {
        self.gpu.forget_custom(entry);
        shaders::forget_entry(entry);
    }

    /// The first frame drawing into `format` (deep colour on, or points): every built-in shader's pipeline for it
    /// is made on a worker, and every imported / user shader built so far, so an effect added later doesn't build
    /// one mid-frame. (What this frame draws is built as it's needed, as before.)
    pub(super) fn prewarm_format(&mut self, format: wgpu::TextureFormat, entries: Vec<&'static str>) {
        if !self.builds.prewarmed.insert(format) {
            return;
        }
        let jobs: Vec<(&'static str, wgpu::ShaderModule)> = entries
            .into_iter()
            .chain(self.gpu.custom_modules.keys().copied())
            .filter(|e| !self.gpu.has_pipeline(e, format))
            .map(|e| (e, self.gpu.custom_modules.get(e).unwrap_or(self.gpu.builtin_module()).clone()))
            .collect();
        let (device, layout) = self.gpu.compile_parts();
        let tx = self.builds.tx.clone();
        spawn(format!("pipelines: {format:?}"), move || {
            for (entry, module) in jobs {
                let pipeline = crate::gpu::build_pipeline(&device, &layout, &module, entry, format);
                if tx.send(Finished::Pipeline { entry, format, module, pipeline }).is_err() {
                    return;
                }
            }
        });
    }

    /// What a frame of `comp` draws into besides the 8-bit mix: made ahead the first time (`prewarm_format`).
    pub(super) fn prewarm_for(&mut self, comp: &Composition) {
        if self.mix_format == crate::gpu::DEEP_FORMAT && !self.builds.prewarmed.contains(&crate::gpu::DEEP_FORMAT) {
            self.prewarm_format(crate::gpu::DEEP_FORMAT, crate::gpu::builtin_entries());
        }
        if !self.builds.prewarmed.contains(&crate::gpu::DATA_FORMAT) && comp.layers.iter().any(|l| l.is_data()) {
            // Points: their generators and effects (16-bit float through the chain), and the copies feeding them on.
            let entries = crate::gpu::builtin_entries().into_iter().filter(|e| e.contains("pts") || matches!(*e, "fs_to_rgb" | "fs_mix" | "fs_composite" | "fs_copy_raw")).collect();
            self.prewarm_format(crate::gpu::DATA_FORMAT, entries);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::gpu_tests::test_engine;
    use super::*;

    const RED: &str = "// @name Red\n@fragment\nfn main(in: VOut) -> @location(0) vec4f {\n    return vec4f(1.0, 0.0, 0.0, 1.0);\n}\n";
    const GREEN: &str = "// @name Red\n@fragment\nfn main(in: VOut) -> @location(0) vec4f {\n    return vec4f(0.0, 1.0, 0.0, 1.0);\n}\n";

    /// Waits for the workers (tests only): renders frames until `done` or ten seconds.
    fn until(engine: &mut Engine, comp: &Composition, done: impl Fn(&mut Engine) -> bool) {
        let started = std::time::Instant::now();
        while !done(engine) && started.elapsed().as_secs() < 10 {
            engine.render(comp, 0.0, &ModInputs::default());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn middle(engine: &mut Engine, comp: &Composition) -> [u8; 4] {
        engine.render(comp, 0.0, &ModInputs::default());
        let (w, h, px) = engine.read_output().expect("output");
        let i = ((h / 2 * w + w / 2) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    /// A user shader compiled on a worker: the old build draws until the new one arrives, a broken edit leaves the
    /// last good one drawing, compiling again makes no new names, and a deleted one's late answer is ignored.
    #[test]
    fn user_shaders_compile_off_the_render_thread() {
        let Some(mut engine) = test_engine() else { return };
        let id = "customgen:async_test";
        let mut comp = Composition { width: 16, height: 16, ..Default::default() };
        comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator(id.into()), ..Default::default() });
        engine.compile_custom_async(id, RED).expect("header");
        assert!(engine.custom_compiling(id));
        until(&mut engine, &comp, |e| !e.custom_compiling(id));
        let done = engine.take_custom_compiled();
        assert_eq!(done.len(), 1);
        let first = done[0].result.as_ref().expect("compiles").entry;
        assert!(middle(&mut engine, &comp)[0] > 250, "red");
        // A new version: until it's in, red still draws (it went blank, or hitched the frame, before).
        engine.compile_custom_async(id, GREEN).expect("header");
        assert!(middle(&mut engine, &comp)[0] > 250 || !engine.custom_compiling(id), "the old build keeps drawing");
        until(&mut engine, &comp, |e| !e.custom_compiling(id));
        let second = engine.take_custom_compiled().pop().unwrap().result.expect("compiles").entry;
        assert_ne!(first, second);
        assert!(!engine.gpu.keeps(first), "the old build is dropped");
        assert!(middle(&mut engine, &comp)[1] > 250, "green");
        // Broken: the error comes back, green keeps drawing.
        engine.compile_custom_async(id, "// @name Red\n@fragment\nfn main(in: VOut) -> @location(0) vec4f {\n    return nope;\n}\n").expect("header");
        until(&mut engine, &comp, |e| !e.custom_compiling(id));
        let err = engine.take_custom_compiled().pop().unwrap().result.expect_err("doesn't compile");
        assert!(err.line.is_some(), "{err}");
        assert!(middle(&mut engine, &comp)[1] > 250, "still green");
        // Back to red: the first name again, nothing new kept.
        engine.compile_custom_async(id, RED).expect("header");
        until(&mut engine, &comp, |e| !e.custom_compiling(id));
        assert_eq!(engine.take_custom_compiled().pop().unwrap().result.expect("compiles").entry, first);
        // Deleted while compiling: its answer is dropped and it stays unlisted.
        engine.compile_custom_async(id, GREEN).expect("header");
        engine.remove_custom(id);
        until(&mut engine, &comp, |e| e.builds.custom_running == 0);
        assert!(engine.take_custom_compiled().is_empty());
        assert!(shaders::generator(id).is_none());
    }

    /// An imported shader whose file changed keeps drawing its old version until the new one is built (it went
    /// blank), then shows the new one.
    #[test]
    fn a_changed_imported_shader_keeps_drawing_until_rebuilt() {
        let Some(mut engine) = test_engine() else { return };
        let entry = shaders::intern("isfgen_changed_test");
        let desc = shaders::intern_desc("isfgen:changed_test", "Changed", entry, &[]);
        let code = |rgb: &str| format!("@fragment\nfn {entry}(in: VOut) -> @location(0) vec4f {{\n    return vec4f({rgb}, 1.0);\n}}\n");
        engine.register_lazy(desc, code("1.0, 0.0, 0.0"), ShaderSetup::default());
        let mut comp = Composition { width: 16, height: 16, ..Default::default() };
        comp.layers.push(LayerState { id: 1, source: SourceSpec::Generator(desc.id.into()), ..Default::default() });
        until(&mut engine, &comp, |e| e.ensure_module(entry) && !e.shader_pending(entry));
        assert!(middle(&mut engine, &comp)[0] > 250, "red");
        engine.replace_lazy(desc, code("0.0, 0.0, 1.0"), ShaderSetup::default());
        let px = middle(&mut engine, &comp);
        assert!(px[0] > 250 || px[2] > 250, "never blank while it rebuilds: {px:?}");
        until(&mut engine, &comp, |e| !e.shader_pending(entry));
        assert!(middle(&mut engine, &comp)[2] > 250, "blue");
    }

    #[test]
    fn compiling_again_keeps_no_new_names() {
        let a = shaders::intern("phosphor_intern_test");
        assert!(std::ptr::eq(a, shaders::intern(&String::from("phosphor_intern_test"))));
        let h = crate::custom::parse_header("// @param Speed 0 4 1\n").unwrap();
        let h2 = crate::custom::parse_header("// @param Speed 0 4 1\n").unwrap();
        assert!(std::ptr::eq(h.params[0].name, h2.params[0].name), "a header read twice: one name");
        let d1 = crate::custom::make_desc("custom:intern_test", &h, a);
        let d2 = crate::custom::make_desc("custom:intern_test", &h2, a);
        assert!(std::ptr::eq(d1, d2), "the same description");
        let h3 = crate::custom::parse_header("// @param Speed 0 8 1\n").unwrap();
        assert!(!std::ptr::eq(d1, crate::custom::make_desc("custom:intern_test", &h3, a)), "other settings: another");
    }
}
