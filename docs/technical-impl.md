Let me explore both the commonplace backend structure and understand how the sync would work.

[3 tools called]

Now let me look at how routes are configured in the main file:

[3 tools called]

Now I have a complete picture. Here's how to implement the synchronization:

## Implementation Plan

### Data Mapping
The Light extension stores highlights like this:
```json
{
  "https://example.com/article": [{
    "chunks": ["highlighted text..."],
    "date": "2025-12-19T13:01:41.308Z",
    "groupID": 1766149301308,
    "repr": "highlighted text...",
    "url": "https://example.com/article"
  }]
}
```

This maps to Commonplace:
- **URL** → `Resource` (title = URL, type = 'website')
- **Highlight** → `Annotation` (text = repr, boundary = {groupID, date, chunks, url})

---

### 1. Backend: Add Sync Endpoint

First, add a bulk sync handler in `src/commonplace/handler.rs`:

```rust
// Add to handler.rs

#[derive(Debug, Deserialize)]
pub struct LightHighlight {
    pub chunks: Vec<String>,
    pub date: String,
    #[serde(rename = "groupID")]
    pub group_id: i64,
    pub repr: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub highlights: std::collections::HashMap<String, Vec<LightHighlight>>,
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub resources_created: i32,
    pub annotations_created: i32,
    pub annotations_skipped: i32,
}

pub async fn sync_highlights(
    State(state): State<AppState>,
    Json(payload): Json<SyncRequest>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());
    
    let mut resources_created = 0;
    let mut annotations_created = 0;
    let mut annotations_skipped = 0;
    
    for (url, highlights) in payload.highlights {
        // Find or create resource for this URL
        let resource_id = match lib.find_resource_by_title(&url).await {
            Ok(Some(r)) => r.id,
            Ok(None) => {
                match lib.create_resource(CreateResource {
                    title: url.clone(),
                    book_title: None,
                    resource_type: ResourceType::Website,
                }).await {
                    Ok(r) => {
                        resources_created += 1;
                        r.id
                    }
                    Err(e) => {
                        tracing::error!("Failed to create resource for {}: {}", url, e);
                        continue;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to find resource {}: {}", url, e);
                continue;
            }
        };
        
        // Create annotations for each highlight
        for highlight in highlights {
            // Check if annotation with this groupID already exists
            if lib.annotation_exists_by_group_id(resource_id, highlight.group_id).await.unwrap_or(false) {
                annotations_skipped += 1;
                continue;
            }
            
            let boundary = serde_json::json!({
                "groupID": highlight.group_id,
                "date": highlight.date,
                "chunks": highlight.chunks,
                "url": highlight.url,
            });
            
            match lib.create_annotation(CreateAnnotation {
                resource_id,
                text: highlight.repr,
                color: Some("yellow".to_string()),
                boundary: Some(boundary),
            }).await {
                Ok(_) => annotations_created += 1,
                Err(e) => {
                    tracing::error!("Failed to create annotation: {}", e);
                }
            }
        }
    }
    
    success(SyncResponse {
        resources_created,
        annotations_created,
        annotations_skipped,
    })
}
```

Add helper methods to `src/commonplace/lib.rs`:

```rust
// Add to lib.rs in impl Commonplace

pub async fn find_resource_by_title(&self, title: &str) -> Result<Option<Resource>> {
    let query = r#"
        SELECT id, title, book_title, type, created_at, updated_at
        FROM resources WHERE title = ?
    "#;

    let mut rows = self.conn.query(query, libsql::params![title]).await?;

    if let Some(row) = rows.next().await? {
        Ok(Some(self.row_to_resource(&row)?))
    } else {
        Ok(None)
    }
}

pub async fn annotation_exists_by_group_id(&self, resource_id: i32, group_id: i64) -> Result<bool> {
    let query = r#"
        SELECT 1 FROM annotations 
        WHERE resource_id = ? 
        AND json_extract(boundary, '$.groupID') = ?
        LIMIT 1
    "#;

    let mut rows = self.conn.query(query, libsql::params![resource_id, group_id]).await?;
    Ok(rows.next().await?.is_some())
}
```

Add the route in `src/commonplace/routes.rs`:

```rust
// Add to routes()
.route("/sync", post(handler::sync_highlights))
```

---

### 2. Extension: Add Sync Button

Update **popup.html** to add a sync button:

```html
<!-- Add to popup.html -->
<button id="syncBtn" class="sync-btn">
  <span class="sync-icon">↻</span> Sync to Commonplace
</button>
<div id="syncStatus"></div>
```

Update **popup.js** to handle sync:

```javascript
// Configuration - user can change this
const COMMONPLACE_API = 'http://localhost:3000/commonplace';

// Sync button handler
document.getElementById('syncBtn').addEventListener('click', syncToCommonplace);

async function syncToCommonplace() {
  const statusEl = document.getElementById('syncStatus');
  const btn = document.getElementById('syncBtn');
  
  try {
    btn.disabled = true;
    statusEl.textContent = 'Syncing...';
    statusEl.className = 'status syncing';
    
    // Get all highlights from storage
    const highlights = await new Promise((resolve) => {
      chrome.storage.local.get(null, resolve);
    });
    
    // Send to commonplace API
    const response = await fetch(`${COMMONPLACE_API}/sync`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ highlights }),
    });
    
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    
    const result = await response.json();
    const data = result.data;
    
    statusEl.textContent = `✓ Synced! ${data.annotations_created} new, ${data.annotations_skipped} existing`;
    statusEl.className = 'status success';
    
    // Store last sync time
    chrome.storage.local.set({ 
      _lastSync: new Date().toISOString() 
    });
    
  } catch (error) {
    statusEl.textContent = `✗ Sync failed: ${error.message}`;
    statusEl.className = 'status error';
  } finally {
    btn.disabled = false;
  }
}

// Optional: Periodic sync (every 30 minutes)
async function setupPeriodicSync() {
  chrome.alarms.create('commonplaceSync', { periodInMinutes: 30 });
}

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === 'commonplaceSync') {
    syncToCommonplace();
  }
});
```

Update **popup.css** for styling:

```css
.sync-btn {
  width: 100%;
  padding: 10px;
  margin-top: 10px;
  background: #4a90d9;
  color: white;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}

.sync-btn:hover {
  background: #3a7bc8;
}

.sync-btn:disabled {
  background: #999;
  cursor: not-allowed;
}

.sync-icon {
  font-size: 16px;
}

.status {
  margin-top: 8px;
  padding: 8px;
  border-radius: 4px;
  font-size: 12px;
  text-align: center;
}

.status.syncing {
  background: #fff3cd;
  color: #856404;
}

.status.success {
  background: #d4edda;
  color: #155724;
}

.status.error {
  background: #f8d7da;
  color: #721c24;
}
```

Update **manifest.json** to add permissions:

```json
{
  "permissions": [
    "storage",
    "alarms"
  ],
  "host_permissions": [
    "http://localhost:*/*"
  ]
}
```

---

### 3. CORS Configuration (Backend)

You'll need to enable CORS on your backend. Add to `main.rs`:

```rust
use tower_http::cors::{Any, CorsLayer};

// In your Router setup:
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_headers(Any);

let app = Router::new()
    // ... routes ...
    .layer(cors)
    .with_state(AppState { db, s3 });
```

---

### Summary

| Component | Changes |
|-----------|---------|
| `handler.rs` | Add `sync_highlights` handler + DTOs |
| `lib.rs` | Add `find_resource_by_title`, `annotation_exists_by_group_id` |
| `routes.rs` | Add `/sync` POST route |
| `popup.html` | Add sync button + status div |
| `popup.js` | Add sync logic + optional periodic sync |
| `popup.css` | Style sync button + status messages |
| `manifest.json` | Add `alarms` permission + host permissions |
| `main.rs` | Add CORS layer |

Would you like me to implement these changes in the codebase?