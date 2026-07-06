# Path Handling

This project should prefer normal absolute paths in user-facing CLI output and errors.

Example:

```text
C:\Users\yuyue\Pictures\Timelapse\sessions\2026-06-29_220503
```

## Normal Absolute Paths

Pros:

- Easier to read in terminal output.
- Matches what users type in PowerShell.
- Easier to copy into Explorer, editors, logs, and docs.
- Better default UX for a CLI tool.

Cons:

- May not be fully normalized.
- Can still contain symlinks, junctions, `..`, or unusual casing.
- Two different-looking paths may point to the same file.
- Historically more exposed to Windows path-length behavior in some APIs.

## Windows Extended-Length Paths

Rust's `std::fs::canonicalize()` can produce Windows extended-length paths.

Example:

```text
\\?\C:\Users\yuyue\Pictures\Timelapse\sessions\2026-06-29_220503
```

Pros:

- Fully resolved by the filesystem.
- Removes ambiguity from relative paths, symlinks, junctions, and `..`.
- Can avoid some older Windows path-length limits.
- Useful internally when strict filesystem identity matters.

Cons:

- Noisy and surprising in CLI output.
- Less pleasant to copy and paste.
- Some external tools, scripts, and libraries may handle it less gracefully.
- Makes logs and errors feel lower-level than this product needs.

## Recommendation

Use normal absolute paths for user-facing render plans, success messages, and errors.

Use extended-length paths only internally when a feature specifically needs strict path identity or filesystem normalization.

For the current renderer, `absolute_from_current()` is usually a better default than `canonicalize()` because ffmpeg and users both work well with normal absolute paths.

## Example implementation

```diff
diff --git a/src/render.rs b/src/render.rs
index 64b7656..3bba2dd 100644
--- a/src/render.rs
+++ b/src/render.rs
@@ -338,7 +338,7 @@ fn resolve_path_target(path: PathBuf) -> Result<ResolvedRenderTarget> {
         });
     }
 
-    let target_path = absolute_from_current(path)?;
+    let target_path = path.canonicalize()?;
     let session_frames_dir = target_path.join("frames");
     if session_frames_dir.is_dir() {
         return Ok(ResolvedRenderTarget {
@@ -473,8 +473,14 @@ mod tests {
         .unwrap();
 
         assert_eq!(plan.source_kind, RenderSourceKind::Session);
-        assert_eq!(plan.sequence.frames_dir, frames);
-        assert_eq!(plan.output_path, session.join("2026-06-29_220503.mp4"));
+        assert_eq!(plan.sequence.frames_dir, frames.canonicalize().unwrap());
+        assert_eq!(
+            plan.output_path,
+            session
+                .canonicalize()
+                .unwrap()
+                .join("2026-06-29_220503.mp4")
+        );
     }
 
     #[test]
@@ -495,7 +501,10 @@ mod tests {
 
         assert_eq!(plan.source_kind, RenderSourceKind::FramesDirectory);
         assert_eq!(plan.sequence.input_pattern(), "%04d.png");
-        assert_eq!(plan.output_path, frames.join("frames.mp4"));
+        assert_eq!(
+            plan.output_path,
+            frames.canonicalize().unwrap().join("frames.mp4")
+        );
         assert_eq!(plan.fps, 24);
     }
```
