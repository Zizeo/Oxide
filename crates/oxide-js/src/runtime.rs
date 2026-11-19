//! JavaScript runtime embedding Boa and exposing DOM / browser bindings.

use boa_engine::gc::{Finalize, Trace};
use boa_engine::js_string;
use boa_engine::native_function::NativeFunction;
use boa_engine::object::JsObject;
use boa_engine::property::{Attribute, PropertyDescriptor};
use boa_engine::{Context, JsValue, Source};
use oxide_dom::{Document, NodeId};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

#[derive(Clone, Trace, Finalize)]
struct DomContextRef {
    #[unsafe_ignore_trace]
    doc: Arc<Mutex<Document>>,
    #[unsafe_ignore_trace]
    mutated: Arc<AtomicBool>,
    #[unsafe_ignore_trace]
    console_logs: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone, Trace, Finalize)]
struct ElementRef {
    #[unsafe_ignore_trace]
    doc: Arc<Mutex<Document>>,
    #[unsafe_ignore_trace]
    node_id: NodeId,
    #[unsafe_ignore_trace]
    mutated: Arc<AtomicBool>,
}

/// A JavaScript execution runtime for a browser tab.
pub struct JsRuntime {
    context: Context,
    doc: Arc<Mutex<Document>>,
    mutated: Arc<AtomicBool>,
    console_logs: Arc<Mutex<Vec<String>>>,
}

impl JsRuntime {
    /// Creates and initializes a new JS runtime with `console` and `document` DOM bindings.
    #[must_use]
    pub fn new(
        doc: Arc<Mutex<Document>>,
        console_logs: Arc<Mutex<Vec<String>>>,
        mutated: Arc<AtomicBool>,
    ) -> Self {
        let mut context = Context::default();
        context.runtime_limits_mut().set_recursion_limit(64);
        context.runtime_limits_mut().set_stack_size_limit(512);
        context.runtime_limits_mut().set_loop_iteration_limit(50_000);

        let dom_ref = DomContextRef {
            doc: doc.clone(),
            mutated: mutated.clone(),
            console_logs: console_logs.clone(),
        };

        // 1. Initialize `console` object
        setup_console(&mut context, dom_ref.clone());

        // 2. Initialize `document` object
        setup_document(&mut context, dom_ref);

        // 3. Install Web APIs (EventTarget, Timers, Storage, URL, History, Fetch, Canvas 2D)
        if let Err(e) = context.eval(Source::from_bytes(crate::web_apis::WEB_APIS_JS)) {
            panic!("WEB_APIS_JS failed: {e}");
        }

        Self {
            context,
            doc,
            mutated,
            console_logs,
        }
    }

    /// Advances JS timers by `dt_ms` and drains the Promise microtask job queue.
    pub fn tick(&mut self, dt_ms: u64) {
        let _ = self.eval(&format!("this.__oxideTick && this.__oxideTick({dt_ms});"));
        let _ = self.context.run_jobs();
    }

    /// Dispatches a DOM event (`click`, `keydown`, `DOMContentLoaded`, etc.)
    /// through the 3-phase Capture -> Target -> Bubble flow.
    pub fn dispatch_dom_event(&mut self, node_id: Option<NodeId>, event_type: &str) -> bool {
        let script = if let Some(nid) = node_id {
            let raw_id = oxide_dom::node_key_id(nid);
            format!(
                "(function(){{ var ev = new Event({event_type:?}, {{bubbles:true,cancelable:true}}); var el = globalThis.__oxideNodes && globalThis.__oxideNodes.get({raw_id}); return el ? el.dispatchEvent(ev) : document.dispatchEvent(ev); }})()"
            )
        } else {
            format!(
                "(function(){{ var ev = new Event({event_type:?}, {{bubbles:true,cancelable:true}}); return document.dispatchEvent(ev); }})()"
            )
        };
        self.eval(&script).map(|s| s == "true").unwrap_or(true)
    }

    /// Evaluates a JavaScript script string, returning the result or error string.
    pub fn eval(&mut self, script: &str) -> Result<String, String> {
        let source = Source::from_bytes(script);
        match self.context.eval(source) {
            Ok(val) => {
                let res_str = val
                    .to_string(&mut self.context)
                    .map_or_else(|_| "<value>".to_string(), |s| s.to_std_string_escaped());
                Ok(res_str)
            }
            Err(err) => {
                let err_str = err.to_string();
                error!(error = %err_str, "JavaScript runtime execution error");
                if let Ok(mut logs) = self.console_logs.lock() {
                    logs.push(format!("[JS ERROR] {err_str}"));
                }
                Err(err_str)
            }
        }
    }

    /// Finds all `<script>` tags in the document, executes their contents, and returns whether DOM was mutated.
    pub fn execute_document_scripts(&mut self) -> bool {
        const MAX_INLINE_SCRIPT_BYTES: usize = 4096;

        let scripts: Vec<String> = {
            if let Ok(doc) = self.doc.lock() {
                let script_nodes = doc.find_elements_by_tag("script");
                script_nodes
                    .into_iter()
                    .map(|id| doc.text_content(id))
                    .filter(|text| {
                        let trimmed = text.trim();
                        !trimmed.is_empty() && trimmed.len() <= MAX_INLINE_SCRIPT_BYTES
                    })
                    .collect()
            } else {
                Vec::new()
            }
        };

        for script in scripts {
            info!("Executing document <script> block");
            let _ = self.eval(&script);
        }

        self.mutated.swap(false, Ordering::SeqCst)
    }

    /// Checks and clears the DOM mutation flag.
    pub fn take_mutation(&self) -> bool {
        self.mutated.swap(false, Ordering::SeqCst)
    }
}

fn setup_console(context: &mut Context, dom_ref: DomContextRef) {
    let console = JsObject::with_object_proto(context.intrinsics());

    // console.log
    let log_ref = dom_ref.clone();
    let log_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let mut parts = Vec::new();
            for arg in args {
                let s = arg
                    .to_string(ctx)
                    .map_or_else(|_| String::new(), |js_str| js_str.to_std_string_escaped());
                parts.push(s);
            }
            let message = parts.join(" ");
            info!(target: "js_console", "{}", message);
            if let Ok(mut logs) = cap.console_logs.lock() {
                logs.push(format!("[LOG] {message}"));
            }
            Ok(JsValue::undefined())
        },
        log_ref,
    );

    // console.warn
    let warn_ref = dom_ref.clone();
    let warn_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let mut parts = Vec::new();
            for arg in args {
                let s = arg
                    .to_string(ctx)
                    .map_or_else(|_| String::new(), |js_str| js_str.to_std_string_escaped());
                parts.push(s);
            }
            let message = parts.join(" ");
            warn!(target: "js_console", "{}", message);
            if let Ok(mut logs) = cap.console_logs.lock() {
                logs.push(format!("[WARN] {message}"));
            }
            Ok(JsValue::undefined())
        },
        warn_ref,
    );

    // console.error
    let err_ref = dom_ref;
    let err_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let mut parts = Vec::new();
            for arg in args {
                let s = arg
                    .to_string(ctx)
                    .map_or_else(|_| String::new(), |js_str| js_str.to_std_string_escaped());
                parts.push(s);
            }
            let message = parts.join(" ");
            error!(target: "js_console", "{}", message);
            if let Ok(mut logs) = cap.console_logs.lock() {
                logs.push(format!("[ERROR] {message}"));
            }
            Ok(JsValue::undefined())
        },
        err_ref,
    );

    let realm = context.realm().clone();
    let _ = console.set(
        js_string!("log"),
        log_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = console.set(
        js_string!("warn"),
        warn_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = console.set(
        js_string!("error"),
        err_fn.to_js_function(&realm),
        false,
        context,
    );

    let _ = context.register_global_property(js_string!("console"), console, Attribute::all());
}

fn setup_document(context: &mut Context, dom_ref: DomContextRef) {
    let document = JsObject::with_object_proto(context.intrinsics());

    // document.getElementById(id)
    let get_id_ref = dom_ref.clone();
    let get_elem_by_id_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let id_str = match args.first().and_then(|v| v.as_string()) {
                Some(s) => s.to_std_string_escaped(),
                None => return Ok(JsValue::null()),
            };

            let node_id = {
                let doc = cap.doc.lock().map_err(|e| {
                    boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
                })?;
                doc.get_element_by_id(&id_str)
            };

            match node_id {
                Some(nid) => Ok(JsValue::new(create_element_object(
                    ctx,
                    cap.doc.clone(),
                    nid,
                    cap.mutated.clone(),
                ))),
                None => Ok(JsValue::null()),
            }
        },
        get_id_ref,
    );

    // document.querySelector(selector)
    let qsa_ref = dom_ref.clone();
    let query_selector_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let sel_str = match args.first().and_then(|v| v.as_string()) {
                Some(s) => s.to_std_string_escaped(),
                None => return Ok(JsValue::null()),
            };

            let node_id = {
                let doc = cap.doc.lock().map_err(|e| {
                    boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
                })?;
                doc.query_selector(&sel_str)
            };

            match node_id {
                Some(nid) => Ok(JsValue::new(create_element_object(
                    ctx,
                    cap.doc.clone(),
                    nid,
                    cap.mutated.clone(),
                ))),
                None => Ok(JsValue::null()),
            }
        },
        qsa_ref,
    );

    // document.title getter & setter
    let title_ref_get = dom_ref.clone();
    let get_title_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap, _ctx| {
            let doc = cap.doc.lock().map_err(|e| {
                boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
            })?;
            let title = doc.title().unwrap_or_default();
            Ok(JsValue::new(js_string!(title)))
        },
        title_ref_get,
    );

    let title_ref_set = dom_ref.clone();
    let set_title_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let new_title = args.first().map_or_else(
                || Ok(String::new()),
                |v| v.to_string(ctx).map(|s| s.to_std_string_escaped()),
            )?;

            let mut doc = cap.doc.lock().map_err(|e| {
                boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
            })?;

            let title_nodes = doc.find_elements_by_tag("title");
            if let Some(&title_id) = title_nodes.first() {
                doc.set_text_content(title_id, &new_title);
            }
            cap.mutated.store(true, Ordering::SeqCst);
            Ok(JsValue::undefined())
        },
        title_ref_set,
    );

    let qsa_all_ref = dom_ref.clone();
    let query_selector_all_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let sel = match args.first().and_then(|v| v.as_string()) {
                Some(s) => s.to_std_string_escaped(),
                None => return Ok(JsValue::new(boa_engine::object::builtins::JsArray::new(ctx))),
            };
            let ids = {
                let doc = cap.doc.lock().map_err(|e| {
                    boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
                })?;
                doc.query_selector_all(&sel)
            };
            let arr = boa_engine::object::builtins::JsArray::new(ctx);
            for nid in ids {
                let el = create_element_object(ctx, cap.doc.clone(), nid, cap.mutated.clone());
                let _ = arr.push(JsValue::new(el), ctx);
            }
            Ok(JsValue::new(arr))
        },
        qsa_all_ref,
    );

    let by_cls_ref = dom_ref.clone();
    let get_by_class_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let cls = match args.first().and_then(|v| v.as_string()) {
                Some(s) => s.to_std_string_escaped(),
                None => return Ok(JsValue::new(boa_engine::object::builtins::JsArray::new(ctx))),
            };
            let ids = {
                let doc = cap.doc.lock().map_err(|e| {
                    boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
                })?;
                doc.find_elements_by_class(&cls)
            };
            let arr = boa_engine::object::builtins::JsArray::new(ctx);
            for nid in ids {
                let el = create_element_object(ctx, cap.doc.clone(), nid, cap.mutated.clone());
                let _ = arr.push(JsValue::new(el), ctx);
            }
            Ok(JsValue::new(arr))
        },
        by_cls_ref,
    );

    let by_tag_ref = dom_ref.clone();
    let get_by_tag_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let tag = match args.first().and_then(|v| v.as_string()) {
                Some(s) => s.to_std_string_escaped(),
                None => return Ok(JsValue::new(boa_engine::object::builtins::JsArray::new(ctx))),
            };
            let ids = {
                let doc = cap.doc.lock().map_err(|e| {
                    boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
                })?;
                doc.find_elements_by_tag(&tag)
            };
            let arr = boa_engine::object::builtins::JsArray::new(ctx);
            for nid in ids {
                let el = create_element_object(ctx, cap.doc.clone(), nid, cap.mutated.clone());
                let _ = arr.push(JsValue::new(el), ctx);
            }
            Ok(JsValue::new(arr))
        },
        by_tag_ref,
    );

    let create_el_ref = dom_ref.clone();
    let create_element_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let tag = match args.first().and_then(|v| v.as_string()) {
                Some(s) => s.to_std_string_escaped(),
                None => return Ok(JsValue::null()),
            };
            let nid = {
                let mut doc = cap.doc.lock().map_err(|e| {
                    boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
                })?;
                doc.create_element(&tag)
            };
            Ok(JsValue::new(create_element_object(
                ctx,
                cap.doc.clone(),
                nid,
                cap.mutated.clone(),
            )))
        },
        create_el_ref,
    );

    let create_txt_ref = dom_ref.clone();
    let create_text_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap, ctx| {
            let text = args.first().map_or_else(
                || Ok(String::new()),
                |v| v.to_string(ctx).map(|s| s.to_std_string_escaped()),
            )?;
            let nid = {
                let mut doc = cap.doc.lock().map_err(|e| {
                    boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
                })?;
                doc.create_text_node(&text)
            };
            Ok(JsValue::new(create_element_object(
                ctx,
                cap.doc.clone(),
                nid,
                cap.mutated.clone(),
            )))
        },
        create_txt_ref,
    );

    let body_ref = dom_ref;
    let get_body_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap, ctx| {
            let body_id = {
                let doc = cap.doc.lock().map_err(|e| {
                    boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
                })?;
                doc.find_elements_by_tag("body").into_iter().next()
            };
            match body_id {
                Some(nid) => Ok(JsValue::new(create_element_object(
                    ctx,
                    cap.doc.clone(),
                    nid,
                    cap.mutated.clone(),
                ))),
                None => Ok(JsValue::null()),
            }
        },
        body_ref,
    );

    let realm = context.realm().clone();
    let _ = document.set(
        js_string!("getElementById"),
        get_elem_by_id_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = document.set(
        js_string!("querySelector"),
        query_selector_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = document.set(
        js_string!("querySelectorAll"),
        query_selector_all_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = document.set(
        js_string!("getElementsByClassName"),
        get_by_class_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = document.set(
        js_string!("getElementsByTagName"),
        get_by_tag_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = document.set(
        js_string!("createElement"),
        create_element_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = document.set(
        js_string!("createTextNode"),
        create_text_fn.to_js_function(&realm),
        false,
        context,
    );
    let body_desc = PropertyDescriptor::builder()
        .get(get_body_fn.to_js_function(&realm))
        .enumerable(true)
        .configurable(true);
    let _ = document.define_property_or_throw(js_string!("body"), body_desc, context);

    let title_desc = PropertyDescriptor::builder()
        .get(get_title_fn.to_js_function(&realm))
        .set(set_title_fn.to_js_function(&realm))
        .enumerable(true)
        .configurable(true);
    let _ = document.define_property_or_throw(js_string!("title"), title_desc, context);

    let _ = context.register_global_property(js_string!("document"), document.clone(), Attribute::all());

    // Register `window` and `globalThis` global objects
    let window = JsObject::with_object_proto(context.intrinsics());
    let _ = window.set(js_string!("document"), document, false, context);
    let _ = window.set(js_string!("innerWidth"), JsValue::new(1280), false, context);
    let _ = window.set(js_string!("innerHeight"), JsValue::new(800), false, context);

    let noop_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let _ = window.set(
        js_string!("addEventListener"),
        noop_fn.clone().to_js_function(&realm),
        false,
        context,
    );
    let _ = window.set(
        js_string!("removeEventListener"),
        noop_fn.to_js_function(&realm),
        false,
        context,
    );

    let _ = context.register_global_property(js_string!("window"), window.clone(), Attribute::all());
    let _ = context.register_global_property(js_string!("globalThis"), window, Attribute::all());
}

fn create_element_object(
    context: &mut Context,
    doc: Arc<Mutex<Document>>,
    node_id: NodeId,
    mutated: Arc<AtomicBool>,
) -> JsObject {
    let elem_obj = JsObject::with_object_proto(context.intrinsics());
    let elem_ref = ElementRef {
        doc,
        node_id,
        mutated,
    };

    let realm = context.realm().clone();

    // element.getAttribute(name)
    let get_attr_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, _ctx| {
            let name = match args.first().and_then(|v| v.as_string()) {
                Some(s) => s.to_std_string_escaped(),
                None => return Ok(JsValue::null()),
            };

            let doc = elem.doc.lock().map_err(|e| {
                boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
            })?;

            if let Some(node) = doc.get(elem.node_id)
                && let Some(val) = node.attribute(&name)
            {
                return Ok(JsValue::new(js_string!(val)));
            }

            Ok(JsValue::null())
        },
        elem_ref.clone(),
    );

    // element.setAttribute(name, value)
    let set_attr_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, ctx| {
            let name = match args.first().and_then(|v| v.as_string()) {
                Some(s) => s.to_std_string_escaped(),
                None => return Ok(JsValue::undefined()),
            };
            let value = args.get(1).map_or_else(
                || Ok(String::new()),
                |v| v.to_string(ctx).map(|s| s.to_std_string_escaped()),
            )?;

            let mut doc = elem.doc.lock().map_err(|e| {
                boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
            })?;

            doc.set_attribute(elem.node_id, name, value);
            elem.mutated.store(true, Ordering::SeqCst);
            Ok(JsValue::undefined())
        },
        elem_ref.clone(),
    );

    // element.textContent getter & setter
    let get_text_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, elem, _ctx| {
            let doc = elem.doc.lock().map_err(|e| {
                boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
            })?;
            let text = doc.text_content(elem.node_id);
            Ok(JsValue::new(js_string!(text)))
        },
        elem_ref.clone(),
    );

    let set_text_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, ctx| {
            let new_text = args.first().map_or_else(
                || Ok(String::new()),
                |v| v.to_string(ctx).map(|s| s.to_std_string_escaped()),
            )?;

            let mut doc = elem.doc.lock().map_err(|e| {
                boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
            })?;

            doc.set_text_content(elem.node_id, &new_text);
            elem.mutated.store(true, Ordering::SeqCst);
            Ok(JsValue::undefined())
        },
        elem_ref.clone(),
    );

    // element.innerHTML getter & setter
    let get_html_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, elem, _ctx| {
            let doc = elem.doc.lock().map_err(|e| {
                boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
            })?;
            let text = doc.text_content(elem.node_id);
            Ok(JsValue::new(js_string!(text)))
        },
        elem_ref.clone(),
    );

    let set_html_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, ctx| {
            let new_html = args.first().map_or_else(
                || Ok(String::new()),
                |v| v.to_string(ctx).map(|s| s.to_std_string_escaped()),
            )?;

            let mut doc = elem.doc.lock().map_err(|e| {
                boa_engine::JsError::from_opaque(JsValue::new(js_string!(e.to_string())))
            })?;

            doc.set_inner_html(elem.node_id, &new_html);
            elem.mutated.store(true, Ordering::SeqCst);
            Ok(JsValue::undefined())
        },
        elem_ref.clone(),
    );

    // element.tagName getter
    let tag_name = {
        let doc = elem_ref.doc.lock().unwrap();
        doc.get(node_id)
            .and_then(|n| n.tag_name())
            .unwrap_or("DIV")
            .to_ascii_uppercase()
    };

    let _ = elem_obj.set(
        js_string!("getAttribute"),
        get_attr_fn.to_js_function(&realm),
        false,
        context,
    );
    let _ = elem_obj.set(
        js_string!("setAttribute"),
        set_attr_fn.to_js_function(&realm),
        false,
        context,
    );

    let text_desc = PropertyDescriptor::builder()
        .get(get_text_fn.to_js_function(&realm))
        .set(set_text_fn.to_js_function(&realm))
        .enumerable(true)
        .configurable(true);
    let _ = elem_obj.define_property_or_throw(js_string!("textContent"), text_desc, context);

    let html_desc = PropertyDescriptor::builder()
        .get(get_html_fn.to_js_function(&realm))
        .set(set_html_fn.to_js_function(&realm))
        .enumerable(true)
        .configurable(true);
    let _ = elem_obj.define_property_or_throw(js_string!("innerHTML"), html_desc, context);

    let tag_desc = PropertyDescriptor::builder()
        .value(js_string!(tag_name))
        .writable(false)
        .enumerable(true);
    let _ = elem_obj.define_property_or_throw(js_string!("tagName"), tag_desc, context);

    let raw_id = oxide_dom::node_key_id(node_id) as f64;
    let _ = elem_obj.set(js_string!("__nodeId"), JsValue::new(raw_id), false, context);

    let rm_attr_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, _ctx| {
            if let Some(name) = args.first().and_then(|v| v.as_string())
                && let Ok(mut doc) = elem.doc.lock()
            {
                doc.remove_attribute(elem.node_id, &name.to_std_string_escaped());
                elem.mutated.store(true, Ordering::SeqCst);
            }
            Ok(JsValue::undefined())
        },
        elem_ref.clone(),
    );
    let _ = elem_obj.set(
        js_string!("removeAttribute"),
        rm_attr_fn.to_js_function(&realm),
        false,
        context,
    );

    let parent_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, elem, ctx| {
            let pid = elem
                .doc
                .lock()
                .ok()
                .and_then(|d| d.get(elem.node_id)?.parent)
                .filter(|&pid| {
                    elem.doc
                        .lock()
                        .ok()
                        .and_then(|d| d.get(pid).map(|n| n.is_element()))
                        .unwrap_or(false)
                });
            match pid {
                Some(p) => Ok(JsValue::new(create_element_object(
                    ctx,
                    elem.doc.clone(),
                    p,
                    elem.mutated.clone(),
                ))),
                None => Ok(JsValue::null()),
            }
        },
        elem_ref.clone(),
    );
    let parent_desc = PropertyDescriptor::builder()
        .get(parent_fn.to_js_function(&realm))
        .enumerable(true)
        .configurable(true);
    let _ = elem_obj.define_property_or_throw(js_string!("parentNode"), parent_desc, context);

    let append_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, ctx| {
            if let Some(child_obj) = args.first().and_then(|v| v.as_object())
                && let Some(cid) = extract_node_id(&elem.doc, &child_obj, ctx)
                && let Ok(mut doc) = elem.doc.lock()
            {
                doc.append_child(elem.node_id, cid);
                elem.mutated.store(true, Ordering::SeqCst);
            }
            Ok(args.first().cloned().unwrap_or_else(JsValue::null))
        },
        elem_ref.clone(),
    );
    let _ = elem_obj.set(
        js_string!("appendChild"),
        append_fn.to_js_function(&realm),
        false,
        context,
    );

    let insert_before_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, ctx| {
            if let Some(new_obj) = args.first().and_then(|v| v.as_object())
                && let Some(new_id) = extract_node_id(&elem.doc, &new_obj, ctx)
            {
                let ref_id = args
                    .get(1)
                    .and_then(|v| v.as_object())
                    .and_then(|o| extract_node_id(&elem.doc, &o, ctx));
                if let Ok(mut doc) = elem.doc.lock() {
                    doc.insert_before(elem.node_id, new_id, ref_id);
                    elem.mutated.store(true, Ordering::SeqCst);
                }
            }
            Ok(args.first().cloned().unwrap_or_else(JsValue::null))
        },
        elem_ref.clone(),
    );
    let _ = elem_obj.set(
        js_string!("insertBefore"),
        insert_before_fn.to_js_function(&realm),
        false,
        context,
    );

    let remove_child_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, ctx| {
            if let Some(child_obj) = args.first().and_then(|v| v.as_object())
                && let Some(cid) = extract_node_id(&elem.doc, &child_obj, ctx)
                && let Ok(mut doc) = elem.doc.lock()
            {
                doc.remove_child(elem.node_id, cid);
                elem.mutated.store(true, Ordering::SeqCst);
            }
            Ok(args.first().cloned().unwrap_or_else(JsValue::null))
        },
        elem_ref.clone(),
    );
    let _ = elem_obj.set(
        js_string!("removeChild"),
        remove_child_fn.to_js_function(&realm),
        false,
        context,
    );

    let replace_child_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, ctx| {
            if let (Some(new_obj), Some(old_obj)) = (
                args.first().and_then(|v| v.as_object()),
                args.get(1).and_then(|v| v.as_object()),
            ) && let (Some(new_id), Some(old_id)) = (
                extract_node_id(&elem.doc, &new_obj, ctx),
                extract_node_id(&elem.doc, &old_obj, ctx),
            ) && let Ok(mut doc) = elem.doc.lock()
            {
                doc.replace_child(elem.node_id, new_id, old_id);
                elem.mutated.store(true, Ordering::SeqCst);
            }
            Ok(args.get(1).cloned().unwrap_or_else(JsValue::null))
        },
        elem_ref.clone(),
    );
    let _ = elem_obj.set(
        js_string!("replaceChild"),
        replace_child_fn.to_js_function(&realm),
        false,
        context,
    );

    let clone_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, elem, ctx| {
            let deep = args.first().is_some_and(JsValue::to_boolean);
            let cloned = elem.doc.lock().ok().and_then(|mut d| d.clone_node(elem.node_id, deep));
            match cloned {
                Some(cid) => Ok(JsValue::new(create_element_object(
                    ctx,
                    elem.doc.clone(),
                    cid,
                    elem.mutated.clone(),
                ))),
                None => Ok(JsValue::null()),
            }
        },
        elem_ref,
    );
    let _ = elem_obj.set(
        js_string!("cloneNode"),
        clone_fn.to_js_function(&realm),
        false,
        context,
    );

    if let Ok(enhance) = context.global_object().get(js_string!("__oxideEnhanceElement"), context)
        && let Some(func) = enhance.as_callable()
    {
        let _ = func.call(&JsValue::undefined(), &[JsValue::new(elem_obj.clone())], context);
    }

    elem_obj
}

fn extract_node_id(
    doc: &Arc<Mutex<Document>>,
    obj: &JsObject,
    ctx: &mut Context,
) -> Option<NodeId> {
    let val = obj.get(js_string!("__nodeId"), ctx).ok()?;
    let raw = val.as_number()? as u64;
    let d = doc.lock().ok()?;
    d.find_node_by_key_id(raw)
}
