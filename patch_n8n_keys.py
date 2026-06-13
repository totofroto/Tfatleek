import sqlite3
import json

db_path = '/volume1/Docker/n8n/database.sqlite'

print("Connecting to n8n database...")
conn = sqlite3.connect(db_path)
cursor = conn.cursor()

# Get target workflows
cursor.execute("SELECT id, name, nodes FROM workflow_entity WHERE id IN ('ixR5Sr2qS7QdqCsd', '3k0JVu2gV0jLk9mx')")
workflows = cursor.fetchall()

old_token = '3479a4207db839fac337d4aa8f6794d05786f32c'
new_token = '216f9cea96b4dc21117fe7a731be2d7fa83ececb'

old_host = '192.168.254.18:25680'
new_host = '192.168.254.15:25680'

for wf_id, name, nodes_str in workflows:
    nodes = json.loads(nodes_str)
    modified = False
    for node in nodes:
        node_json = json.dumps(node)
        node_modified = False
        
        # Check and replace old token
        if old_token in node_json:
            # Let's do string replacement in headers or params
            node_str = json.dumps(node)
            node_str = node_str.replace(old_token, new_token)
            node = json.loads(node_str)
            node_modified = True
            print(f"[{name}] Replaced old token in node '{node['name']}'")
            
        # Check and replace old host
        if old_host in json.dumps(node):
            node_str = json.dumps(node)
            node_str = node_str.replace(old_host, new_host)
            node = json.loads(node_str)
            node_modified = True
            print(f"[{name}] Replaced old host in node '{node['name']}'")
            
        if node_modified:
            # Put the modified node back into nodes list
            for idx, n in enumerate(nodes):
                if n['id'] == node['id']:
                    nodes[idx] = node
            modified = True

    if modified:
        new_nodes_str = json.dumps(nodes)
        cursor.execute("UPDATE workflow_entity SET nodes = ? WHERE id = ?", (new_nodes_str, wf_id))
        print(f"Saved changes for workflow '{name}' ({wf_id}) to database.")
    else:
        print(f"No changes needed for workflow '{name}' ({wf_id}).")

conn.commit()
conn.close()
print("Keys and hosts patch completed successfully.")
