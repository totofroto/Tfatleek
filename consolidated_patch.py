import sqlite3
import json

db_path = '/volume1/Docker/n8n/database.sqlite'

print("Connecting to n8n database...")
conn = sqlite3.connect(db_path)
cursor = conn.cursor()

# Definitions for updates
old_token = '3479a4207db839fac337d4aa8f6794d05786f32c'
new_token = '216f9cea96b4dc21117fe7a731be2d7fa83ececb'

old_host = '192.168.254.18:25680'
new_host = '192.168.254.15:25680'

# The exact n8n JS object expression requested by the user
new_json_body = '={{ { model: "qwen3:14b", stream: false, messages: [ { role: "system", content: "Du bist Dokumentenklassifikation für eine Arztfamilie. Antworte NUR mit JSON ohne Markdown." }, { role: "user", content: `Titel: ${$json.title || \'\'}\\nInhalt: ${($json.content || \'\').substring(0,1500)}\\n\\nJSON Format:\\n{"title":"Titel DE max 60Z","document_type":"Invoice|Medical|Education|Financial|Household|Employment|Legal & ID|Receipts & Warranties|Vehicles|Empfehlung","correspondent":"Absender oder null","person":"Tareg|Miluda|Fatima|Sama|Family|null","tags":["Rechnung","Medizinisch"],"confidence":"high|medium|low","summary":"1-2 Sätze DE"}` } ], options: { temperature: 0.1 } } }}'

# Query the target workflows
cursor.execute("SELECT id, name, versionId, activeVersionId, nodes FROM workflow_entity WHERE id IN ('ixR5Sr2qS7QdqCsd', '3k0JVu2gV0jLk9mx')")
workflows = cursor.fetchall()

def patch_nodes(nodes, name, context_desc):
    modified = False
    for idx, node in enumerate(nodes):
        node_modified = False
        node_name = node.get('name', '')
        
        # 1. Update classify node parameters (Specify Body & JSON body payload)
        if node_name in ('Classify with qwen3', 'Classify with qwen3 (Local)'):
            print(f"  [{name} - {context_desc}] Patching body of node '{node_name}'")
            node['parameters']['sendBody'] = True
            node['parameters']['specifyBody'] = 'json'
            node['parameters']['jsonBody'] = new_json_body
            if 'bodyParameters' in node['parameters']:
                del node['parameters']['bodyParameters']
            node_modified = True
            
        # 2. Replace token
        node_json_str = json.dumps(node)
        if old_token in node_json_str:
            print(f"  [{name} - {context_desc}] Replacing old token in node '{node_name}'")
            node_json_str = node_json_str.replace(old_token, new_token)
            node = json.loads(node_json_str)
            node_modified = True
            
        # 3. Replace host URL
        if old_host in json.dumps(node):
            print(f"  [{name} - {context_desc}] Replacing old host URL in node '{node_name}'")
            node_json_str = json.dumps(node)
            node_json_str = node_json_str.replace(old_host, new_host)
            node = json.loads(node_json_str)
            node_modified = True
            
        if node_modified:
            nodes[idx] = node
            modified = True
            
    return nodes, modified

for wf_id, name, versionId, activeVersionId, nodes_str in workflows:
    print(f"\nProcessing Workflow '{name}' ({wf_id}):")
    print(f"  Draft versionId: {versionId}")
    print(f"  Active versionId: {activeVersionId}")
    
    # 1. Patch the main draft in workflow_entity
    nodes = json.loads(nodes_str)
    patched_nodes, modified = patch_nodes(nodes, name, "Draft in workflow_entity")
    if modified:
        cursor.execute("UPDATE workflow_entity SET nodes = ? WHERE id = ?", (json.dumps(patched_nodes), wf_id))
        print(f"  -> Updated draft in workflow_entity.")
    else:
        print(f"  -> No changes needed in draft workflow_entity.")
        
    # 2. Patch the matching versions in workflow_history
    target_versions = [v for v in (versionId, activeVersionId) if v]
    if target_versions:
        # Fetch from history
        placeholders = ','.join('?' for _ in target_versions)
        query = f"SELECT versionId, nodes FROM workflow_history WHERE workflowId = ? AND versionId IN ({placeholders})"
        cursor.execute(query, [wf_id] + target_versions)
        history_rows = cursor.fetchall()
        
        print(f"  Found {len(history_rows)} matching history records.")
        for hist_ver_id, hist_nodes_str in history_rows:
            h_nodes = json.loads(hist_nodes_str)
            patched_h_nodes, h_modified = patch_nodes(h_nodes, name, f"History version {hist_ver_id}")
            if h_modified:
                cursor.execute("UPDATE workflow_history SET nodes = ? WHERE workflowId = ? AND versionId = ?", 
                               (json.dumps(patched_h_nodes), wf_id, hist_ver_id))
                print(f"  -> Updated history version {hist_ver_id}.")
            else:
                print(f"  -> No changes needed in history version {hist_ver_id}.")

conn.commit()
conn.close()
print("\nConsolidated database patch completed successfully.")
