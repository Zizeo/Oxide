//! JavaScript runtime integration via Boa for Oxide.

pub mod runtime;

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
}
