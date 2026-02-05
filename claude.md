some migrations have to be registered in the migrations() function in src/commonplace/mod.rs
endpoints on the frontend application should be added to the vite.config.js. (Eg. The /download endpoint was missing from the Vite dev server proxy configuration. Requests were being served by Vite’s SPA fallback (returning index.html) instead of being proxied to your backend)
