// Content scripts cannot be ES modules, so this one loads the real entry point as one.
// The modules it imports are listed under web_accessible_resources in the manifest.
import(chrome.runtime.getURL("content/main.js")).catch((error) => {
  console.error("Light for liotek could not start on this page:", error);
});
