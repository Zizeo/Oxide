//! JavaScript runtime integration via Boa for Oxide.

pub mod runtime;
pub mod web_apis;

pub use runtime::JsRuntime;

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_dom::Document;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_js_console_and_dom_queries() {
        let html = r#"<!DOCTYPE html>
        <html>
        <head><title>JS Test</title></head>
        <body>
            <h1 id="main-title">Original Title</h1>
            <p class="desc">A paragraph</p>
        </body>
        </html>"#;

        let doc = Arc::new(Mutex::new(Document::parse(html)));
        let logs = Arc::new(Mutex::new(Vec::new()));
        let mutated = Arc::new(AtomicBool::new(false));

        let mut js = JsRuntime::new(doc.clone(), logs.clone(), mutated.clone());

        // Test console.log
        js.eval("console.log('Testing from JS', 42);").unwrap();
        {
            let log_list = logs.lock().unwrap();
            assert_eq!(log_list.len(), 1);
            assert!(log_list[0].contains("Testing from JS 42"));
        }

        // Test document.getElementById & textContent mutation
        js.eval(
            r#"
            let heading = document.getElementById("main-title");
            heading.textContent = "Mutated by JavaScript!";
            heading.setAttribute("data-test", "success");
            "#,
        )
        .unwrap();

        assert!(js.take_mutation());

        // Verify changes reflected in Rust DOM
        {
            let d = doc.lock().unwrap();
            let h1 = d.get_element_by_id("main-title").unwrap();
            assert_eq!(d.text_content(h1), "Mutated by JavaScript!");
            assert_eq!(d.get(h1).unwrap().attribute("data-test"), Some("success"));
        }
    }

    #[test]
    fn test_js_script_tag_execution() {
        let html = r#"<!DOCTYPE html>
        <html>
        <body>
            <div id="counter">Initial</div>
            <script>
                console.log("Script block executed");
                let elem = document.getElementById("counter");
                elem.textContent = "Script Ran Successfully";
            </script>
        </body>
        </html>"#;

        let doc = Arc::new(Mutex::new(Document::parse(html)));
        let logs = Arc::new(Mutex::new(Vec::new()));
        let mutated = Arc::new(AtomicBool::new(false));

        let mut js = JsRuntime::new(doc.clone(), logs.clone(), mutated);

        let did_mutate = js.execute_document_scripts();
        assert!(did_mutate);

        {
            let d = doc.lock().unwrap();
            let div_id = d.get_element_by_id("counter").unwrap();
            assert_eq!(d.text_content(div_id), "Script Ran Successfully");
        }

        {
            let log_list = logs.lock().unwrap();
            assert!(log_list.iter().any(|l| l.contains("Script block executed")));
        }
    }

    #[test]
    fn test_js_recursion_and_oversized_script_guard() {
        // Build an oversized deeply nested script (> 8KB) that would overflow a 1MB debug stack if parsed
        let mut huge_expr = String::from("document.title = 'Should Be Skipped'; var x = 0");
        for _ in 0..3000 {
            huge_expr.push_str(" + (1");
        }
        for _ in 0..3000 {
            huge_expr.push(')');
        }
        let html = format!("<!DOCTYPE html><html><head><title>Safe</title></head><body><script>{huge_expr}</script></body></html>");

        let doc = Arc::new(Mutex::new(Document::parse(&html)));
        let logs = Arc::new(Mutex::new(Vec::new()));
        let mutated = Arc::new(AtomicBool::new(false));

        let mut js = JsRuntime::new(doc.clone(), logs, mutated);

        // Must skip oversized bundle without overflowing stack or mutating title
        let did_mutate = js.execute_document_scripts();
        assert!(!did_mutate);
        assert_eq!(doc.lock().unwrap().title(), Some("Safe".to_string()));

        // Infinite recursion must return Err cleanly via runtime_limits instead of STATUS_STACK_OVERFLOW
        let rec_res = js.eval("function inf() { return inf() + 1; } inf();");
        assert!(rec_res.is_err());
    }

    #[test]
    fn test_event_dispatch_dom_mutations_timers_and_web_apis() {
        let html = r#"<!DOCTYPE html><html><body>
            <div id="parent"><button id="btn">Go</button></div>
            <canvas id="c"></canvas>
        </body></html>"#;
        let doc = Arc::new(Mutex::new(Document::parse(html)));
        let logs = Arc::new(Mutex::new(Vec::new()));
        let mutated = Arc::new(AtomicBool::new(false));
        let mut js = JsRuntime::new(doc.clone(), logs, mutated);

        // 1. 3-phase Capture -> Target -> Bubble
        let phases = js
            .eval(
                r#"
                var log = [];
                var p = document.getElementById('parent');
                var b = document.getElementById('btn');
                p.addEventListener('click', () => log.push('p-cap'), true);
                b.addEventListener('click', () => log.push('b-tgt'), false);
                p.addEventListener('click', () => log.push('p-bub'), false);
                b.dispatchEvent(new Event('click', { bubbles: true }));
                log.join(',');
                "#,
            )
            .unwrap();
        assert_eq!(phases, "p-cap,b-tgt,p-bub");

        // 2. DOM mutation + classList + dataset + style
        js.eval(
            r#"
            var el = document.createElement('span');
            el.classList.add('a', 'b');
            el.classList.remove('a');
            el.dataset.userId = '7';
            el.style.backgroundColor = 'red';
            el.textContent = 'hi';
            document.body.appendChild(el);
            "#,
        )
        .unwrap();
        {
            let d = doc.lock().unwrap();
            let spans = d.find_elements_by_tag("span");
            assert_eq!(spans.len(), 1);
            let n = d.get(spans[0]).unwrap();
            assert_eq!(n.attribute("class"), Some("b"));
            assert_eq!(n.attribute("data-user-id"), Some("7"));
            assert!(n.attribute("style").unwrap().contains("background-color: red"));
        }

        // 3. Timers + Storage + URL + History + Canvas 2D
        js.eval(
            r#"
            var timerFired = false;
            setTimeout(() => { timerFired = true; }, 10);
            localStorage.setItem('k', 'v');
            document.cookie = 'a=1; path=/';
            history.pushState({ p: 2 }, '', '/next');
            var ctx = document.getElementById('c').getContext('2d');
            ctx.fillRect(0, 0, 10, 10);
            "#,
        )
        .unwrap();
        js.tick(20);
        assert_eq!(js.eval("timerFired").unwrap(), "true");
        assert_eq!(js.eval("localStorage.getItem('k')").unwrap(), "v");
        assert_eq!(js.eval("document.cookie").unwrap(), "a=1");
        assert_eq!(js.eval("location.href").unwrap(), "/next");
        assert_eq!(js.eval("new URL('https://a.com/p?x=1').searchParams.get('x')").unwrap(), "1");
        assert_eq!(js.eval("document.getElementById('c').getContext('2d').ops.length").unwrap(), "1");
    }
}
