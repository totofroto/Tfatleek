import sqlite3
import json

db_path = '/volume1/Docker/n8n/database.sqlite'

print("Connecting to n8n database...")
conn = sqlite3.connect(db_path)
cursor = conn.cursor()

# Get target workflows
cursor.execute("SELECT id, name, nodes FROM workflow_entity WHERE id IN ('ixR5Sr2qS7QdqCsd', '3k0JVu2gV0jLk9mx')")
workflows = cursor.fetchall()

# The exact n8n JS object expression requested by the user
new_json_body = '={{ { model: "qwen3:14b", stream: false, messages: [ { role: "system", content: "Du bist Dokumentenklassifikation für eine Arztfamilie. Antworte NUR mit JSON ohne Markdown." }, { role: "user", content: `Titel: ${$json.title || \'\'}\\nInhalt: ${($json.content || \'\').substring(0,1500)}\\n\\nJSON Format:\\n{"title":"Titel DE max 60Z","document_type":"Invoice|Medical|Education|Financial|Household|Employment|Legal & ID|Receipts & Warranties|Vehicles|Empfehlung","correspondent":"Absender oder null","person":"Tareg|Miluda|Fatima|Sama|Family|null","tags":["Rechnung","Medizinisch"],"confidence":"high|medium|low","summary":"1-2 Sätze DE"}` } ], options: { temperature: 0.1 } } }}'

for wf_id, name, nodes_str in workflows:
    nodes = json.loads(nodes_str)
    modified = False
    for node in nodes:
        if node.get('name') in ('Classify with qwen3', 'Classify with qwen3 (Local)'):
            print(f"\nFound target node '{node['name']}' in workflow '{name}' ({wf_id})")
            print("OLD parameters:")
            print(json.dumps(node['parameters'], indent=2))
            
            # Update parameters
            node['parameters']['sendBody'] = True
            node['parameters']['specifyBody'] = 'json'
            node['parameters']['jsonBody'] = new_json_body
            if 'bodyParameters' in node['parameters']:
                del node['parameters']['bodyParameters']
                
            print("NEW parameters:")
            print(json.dumps(node['parameters'], indent=2))
            modified = True
            
    if modified:
        # Save changes back to workflow_entity
        new_nodes_str = json.dumps(nodes)
        cursor.execute("UPDATE workflow_entity SET nodes = ? WHERE id = ?", (new_nodes_str, wf_id))
        print(f"Updated workflow '{name}' ({wf_id}) in database.")
    else:
        print(f"No changes made to workflow '{name}' ({wf_id}).")

conn.commit()
conn.close()
print("\nDatabase patch completed successfully.")
