import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SmartGroup {
  id: number;
  name: string;
  icon: string;
  filter_category: string | null;
  filter_tax_relevant: number | null;
  filter_year: number | null;
  filter_correspondent: string | null;
  filter_min_confidence: number;
  created_at: string;
  sort_order: number;
  file_count: number;
}

interface NewSmartGroupForm {
  name: string;
  icon: string;
  filter_category: string;
  filter_tax_relevant: "any" | "yes" | "no";
  filter_year: string;
  filter_correspondent: string;
  filter_min_confidence: number;
}

interface Props {
  selectedGroupId: number;
  onSelect: (group: SmartGroup) => void;
}

const DEFAULT_FORM: NewSmartGroupForm = {
  name: "",
  icon: "📁",
  filter_category: "",
  filter_tax_relevant: "any",
  filter_year: "",
  filter_correspondent: "",
  filter_min_confidence: 0.0,
};

export default function SmartGroupsSidebar({ selectedGroupId, onSelect }: Props) {
  const [groups, setGroups] = useState<SmartGroup[]>([]);
  const [showModal, setShowModal] = useState(false);
  const [form, setForm] = useState<NewSmartGroupForm>(DEFAULT_FORM);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<number | null>(null);

  const loadGroups = useCallback(async () => {
    try {
      const data = await invoke<SmartGroup[]>("get_smart_groups");
      setGroups(data);
    } catch (e) {
      console.error("Failed to load smart groups:", e);
    }
  }, []);

  useEffect(() => {
    loadGroups();
  }, [loadGroups]);

  const handleCreate = async () => {
    if (!form.name.trim()) return;
    setCreating(true);
    setError(null);
    try {
      await invoke("create_smart_group", {
        group: {
          name: form.name.trim(),
          icon: form.icon.trim() || "📁",
          filter_category: form.filter_category.trim() || null,
          filter_tax_relevant:
            form.filter_tax_relevant === "yes"
              ? true
              : form.filter_tax_relevant === "no"
              ? false
              : null,
          filter_year: form.filter_year ? parseInt(form.filter_year, 10) : null,
          filter_correspondent: form.filter_correspondent.trim() || null,
          filter_min_confidence: form.filter_min_confidence,
        },
      });
      setShowModal(false);
      setForm(DEFAULT_FORM);
      await loadGroups();
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (e: React.MouseEvent, groupId: number) => {
    e.stopPropagation();
    try {
      await invoke("delete_smart_group", { groupId });
      await loadGroups();
    } catch (e) {
      alert(String(e));
    }
  };

  return (
    <div className="flex flex-col h-full bg-neutral-950 border-r border-neutral-800 w-56 flex-shrink-0">
      <div className="px-4 py-4 border-b border-neutral-900">
        <span className="text-[10px] font-black uppercase tracking-widest text-neutral-500">
          Smart Groups
        </span>
      </div>

      <div className="flex-1 overflow-y-auto py-2">
        {groups.map((group) => (
          <div
            key={group.id}
            className="relative"
            onMouseEnter={() => setHoveredId(group.id)}
            onMouseLeave={() => setHoveredId(null)}
          >
            <button
              onClick={() => onSelect(group)}
              className={`w-full flex items-center gap-2.5 px-4 py-2.5 text-left transition-all group ${
                selectedGroupId === group.id
                  ? "bg-blue-600/15 border-r-2 border-blue-500"
                  : "hover:bg-neutral-900"
              }`}
            >
              <span className="text-base leading-none">{group.icon}</span>
              <span
                className={`flex-1 text-[11px] font-bold truncate ${
                  selectedGroupId === group.id ? "text-blue-400" : "text-neutral-300"
                }`}
              >
                {group.name}
              </span>
              <div className="flex items-center gap-1.5">
                <span className="text-[10px] bg-neutral-800 text-neutral-500 px-1.5 py-0.5 rounded-full font-mono">
                  {group.file_count}
                </span>
                {group.id <= 4 ? (
                  <span className="text-[10px] text-neutral-700" title="Built-in group">
                    🔒
                  </span>
                ) : hoveredId === group.id ? (
                  <button
                    onClick={(e) => handleDelete(e, group.id)}
                    className="text-[10px] text-neutral-600 hover:text-red-500 transition-colors w-4 h-4 flex items-center justify-center"
                    title="Delete group"
                  >
                    ✕
                  </button>
                ) : (
                  <span className="w-4 h-4" />
                )}
              </div>
            </button>
          </div>
        ))}
      </div>

      <div className="p-3 border-t border-neutral-900">
        <button
          onClick={() => {
            setShowModal(true);
            setError(null);
            setForm(DEFAULT_FORM);
          }}
          className="w-full py-2 rounded-xl bg-neutral-900 hover:bg-neutral-800 border border-neutral-800 text-[10px] font-black uppercase text-neutral-500 hover:text-neutral-300 transition-all"
        >
          ＋ New Group
        </button>
      </div>

      {/* New Group Modal */}
      {showModal && (
        <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50 p-4">
          <div className="bg-neutral-900 border border-neutral-800 rounded-2xl p-6 w-full max-w-md shadow-2xl">
            <h3 className="text-xs font-black uppercase tracking-widest text-blue-400 mb-5">
              New Smart Group
            </h3>

            <div className="space-y-4">
              <div className="grid grid-cols-4 gap-2">
                <div>
                  <label className="text-[10px] text-neutral-500 uppercase font-bold block mb-1.5">
                    Icon
                  </label>
                  <input
                    type="text"
                    value={form.icon}
                    onChange={(e) => setForm({ ...form, icon: e.target.value })}
                    className="w-full bg-black border border-neutral-800 rounded-lg px-2 py-2 text-center text-base outline-none focus:border-blue-500"
                    maxLength={2}
                  />
                </div>
                <div className="col-span-3">
                  <label className="text-[10px] text-neutral-500 uppercase font-bold block mb-1.5">
                    Name *
                  </label>
                  <input
                    type="text"
                    value={form.name}
                    onChange={(e) => setForm({ ...form, name: e.target.value })}
                    placeholder="My Group"
                    className="w-full bg-black border border-neutral-800 rounded-lg px-3 py-2 text-sm font-mono outline-none focus:border-blue-500 text-neutral-200"
                  />
                </div>
              </div>

              <div>
                <label className="text-[10px] text-neutral-500 uppercase font-bold block mb-1.5">
                  Category Filter
                </label>
                <input
                  type="text"
                  value={form.filter_category}
                  onChange={(e) => setForm({ ...form, filter_category: e.target.value })}
                  placeholder="e.g. Medical, Financial (leave empty for all)"
                  className="w-full bg-black border border-neutral-800 rounded-lg px-3 py-2 text-xs font-mono outline-none focus:border-blue-500 text-neutral-300"
                />
              </div>

              <div>
                <label className="text-[10px] text-neutral-500 uppercase font-bold block mb-1.5">
                  Tax Relevant
                </label>
                <div className="flex gap-2">
                  {(["any", "yes", "no"] as const).map((opt) => (
                    <button
                      key={opt}
                      onClick={() => setForm({ ...form, filter_tax_relevant: opt })}
                      className={`flex-1 py-1.5 rounded-lg border text-[10px] font-black uppercase transition-all ${
                        form.filter_tax_relevant === opt
                          ? "bg-blue-600 border-blue-400 text-white"
                          : "bg-black border-neutral-800 text-neutral-500"
                      }`}
                    >
                      {opt}
                    </button>
                  ))}
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-[10px] text-neutral-500 uppercase font-bold block mb-1.5">
                    Year
                  </label>
                  <input
                    type="number"
                    value={form.filter_year}
                    onChange={(e) => setForm({ ...form, filter_year: e.target.value })}
                    placeholder="2024"
                    className="w-full bg-black border border-neutral-800 rounded-lg px-3 py-2 text-xs font-mono outline-none focus:border-blue-500 text-neutral-300"
                  />
                </div>
                <div>
                  <label className="text-[10px] text-neutral-500 uppercase font-bold block mb-1.5">
                    Correspondent
                  </label>
                  <input
                    type="text"
                    value={form.filter_correspondent}
                    onChange={(e) => setForm({ ...form, filter_correspondent: e.target.value })}
                    placeholder="e.g. Finanzamt"
                    className="w-full bg-black border border-neutral-800 rounded-lg px-3 py-2 text-xs font-mono outline-none focus:border-blue-500 text-neutral-300"
                  />
                </div>
              </div>

              <div>
                <label className="text-[10px] text-neutral-500 uppercase font-bold block mb-1.5">
                  Min Confidence:{" "}
                  <span className="text-blue-400">{form.filter_min_confidence.toFixed(2)}</span>
                </label>
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  value={form.filter_min_confidence}
                  onChange={(e) =>
                    setForm({ ...form, filter_min_confidence: parseFloat(e.target.value) })
                  }
                  className="w-full accent-blue-500"
                />
                <div className="flex justify-between text-[9px] text-neutral-600 mt-0.5">
                  <span>0.0 (all)</span>
                  <span>1.0 (perfect)</span>
                </div>
              </div>
            </div>

            {error && (
              <p className="mt-3 text-[10px] text-red-400 font-mono bg-red-900/10 border border-red-900/30 rounded-lg px-3 py-2">
                {error}
              </p>
            )}

            <div className="mt-5 flex justify-end gap-2">
              <button
                onClick={() => setShowModal(false)}
                className="px-5 py-2 text-neutral-500 text-[10px] font-black uppercase"
              >
                Cancel
              </button>
              <button
                onClick={handleCreate}
                disabled={creating || !form.name.trim()}
                className="px-6 py-2 bg-blue-600 hover:bg-blue-500 text-white text-[10px] font-black uppercase rounded-xl transition-all disabled:opacity-50"
              >
                {creating ? "Creating..." : "Create Group"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
