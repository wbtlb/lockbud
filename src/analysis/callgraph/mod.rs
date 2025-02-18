//! Generate a CallGraph for instances in each crate.
//! You can roughly think of instances as a monomorphic function.
//! If an instance calls another instance, then we have an edge
//! from caller to callee with callsite locations as edge weight.
//! This is a fundamental analysis for other analysis,
//! e.g., points-to analysis, lockguard collector, etc.
use petgraph::algo;
use petgraph::dot::{Config, Dot};
use petgraph::graph::NodeIndex;
use petgraph::visit::IntoNodeReferences;
use petgraph::{Directed, Graph};

use rustc_middle::mir::visit::Visitor;
use rustc_middle::mir::{Body, Location, Terminator, TerminatorKind};
use rustc_middle::ty::{self, Instance, ParamEnv, TyCtxt};

/// The NodeIndex in CallGraph, denoting a unique instance in CallGraph.
pub type InstanceId = NodeIndex;

/// The location where caller calls callee.
/// Support direct call for now, where callee resolves to FnDef.
/// TODO(boqin): Add support for FnPtr.
#[derive(Copy, Clone, Debug)]
pub enum CallSiteLocation {
    FnDef(Location),
    // FnPtr(Location),
}

/// CallGraph
/// The nodes of CallGraph are instances.
/// The directed edges are CallSite Locations.
/// e.g., `Instance1--|[CallSite1, CallSite2]|-->Instance2`
/// denotes `Instance1` calls `Instance2` at locations `Callsite1` and `CallSite2`.
pub struct CallGraph<'tcx> {
    pub graph: Graph<Instance<'tcx>, Vec<CallSiteLocation>, Directed>,
}

impl<'tcx> CallGraph<'tcx> {
    /// Create an empty CallGraph.
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
        }
    }

    /// Search for the InstanceId of a given instance in CallGraph.
    pub fn instance_to_index(&self, instance: &Instance<'tcx>) -> Option<InstanceId> {
        self.graph
            .node_references()
            .find(|(_idx, inst)| *inst == instance)
            .map(|(idx, _)| idx)
    }

    /// Get the instance by InstanceId.
    pub fn index_to_instance(&self, idx: InstanceId) -> Option<&Instance<'tcx>> {
        self.graph.node_weight(idx)
    }

    /// Perform callgraph analysis on the given instances.
    /// The instances should be **all** the instances with MIR available in the current crate.
    pub fn analyze(
        &mut self,
        instances: Vec<Instance<'tcx>>,
        tcx: TyCtxt<'tcx>,
        param_env: ParamEnv<'tcx>,
    ) {
        let idx_insts = instances
            .into_iter()
            .map(|inst| {
                let idx = self.graph.add_node(inst);
                (idx, inst)
            })
            .collect::<Vec<_>>();
        for (caller_idx, caller) in idx_insts {
            let body = tcx.instance_mir(caller.def);
            // Skip promoted src
            if body.source.promoted.is_some() {
                continue;
            }
            let mut collector = CallSiteCollector::new(caller, body, tcx, param_env);
            collector.visit_body(body);
            for (callee, location) in collector.finish() {
                let callee_idx = if let Some(callee_idx) = self.instance_to_index(&callee) {
                    callee_idx
                } else {
                    continue;
                };
                if let Some(edge_idx) = self.graph.find_edge(caller_idx, callee_idx) {
                    // Update edge weight.
                    self.graph.edge_weight_mut(edge_idx).unwrap().push(location);
                } else {
                    // Add edge if not exists.
                    self.graph.add_edge(caller_idx, callee_idx, vec![location]);
                }
            }
        }
    }

    /// Find the callsite locations (weight) on the edge from source to target.
    pub fn callsites(
        &self,
        source: InstanceId,
        target: InstanceId,
    ) -> Option<Vec<CallSiteLocation>> {
        let edge = self.graph.find_edge(source, target)?;
        self.graph.edge_weight(edge).cloned()
    }

    /// Find all simple paths from source to target.
    /// e.g., for one of the paths, `source --> instance1 --> instance2 --> target`,
    /// the return is [source, instance1, instance2, target].
    pub fn all_simple_paths(&self, source: InstanceId, target: InstanceId) -> Vec<Vec<InstanceId>> {
        algo::all_simple_paths::<Vec<_>, _>(&self.graph, source, target, 0, None)
            .collect::<Vec<_>>()
    }

    /// Print the callgraph in dot format.
    #[allow(dead_code)]
    pub fn dot(&self) {
        println!(
            "{:?}",
            Dot::with_config(&self.graph, &[Config::GraphContentOnly])
        );
    }
}

/// Visit Terminator and record callsites (callee + location).
struct CallSiteCollector<'a, 'tcx> {
    caller: Instance<'tcx>,
    body: &'a Body<'tcx>,
    tcx: TyCtxt<'tcx>,
    param_env: ParamEnv<'tcx>,
    callsites: Vec<(Instance<'tcx>, CallSiteLocation)>,
}

impl<'a, 'tcx> CallSiteCollector<'a, 'tcx> {
    fn new(
        caller: Instance<'tcx>,
        body: &'a Body<'tcx>,
        tcx: TyCtxt<'tcx>,
        param_env: ParamEnv<'tcx>,
    ) -> Self {
        Self {
            caller,
            body,
            tcx,
            param_env,
            callsites: Vec::new(),
        }
    }

    /// Consumes `CallSiteCollector` and returns its callsites when finished visiting.
    fn finish(self) -> impl IntoIterator<Item = (Instance<'tcx>, CallSiteLocation)> {
        self.callsites.into_iter()
    }
}

impl<'a, 'tcx> Visitor<'tcx> for CallSiteCollector<'a, 'tcx> {
    /// Resolve direct call.
    /// Inspired by rustc_mir/src/transform/inline.rs#get_valid_function_call.
    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, location: Location) {
        if let TerminatorKind::Call { ref func, .. } = terminator.kind {
            let func_ty = func.ty(self.body, self.tcx);
            // Only after monomorphizing can Instance::resolve work
            let func_ty = self.caller.subst_mir_and_normalize_erasing_regions(
                self.tcx,
                self.param_env,
                func_ty,
            );
            if let ty::FnDef(def_id, substs) = *func_ty.kind() {
                if let Some(callee) = Instance::resolve(self.tcx, self.param_env, def_id, substs)
                    .ok()
                    .flatten()
                {
                    self.callsites
                        .push((callee, CallSiteLocation::FnDef(location)));
                }
            }
        }
    }
}
