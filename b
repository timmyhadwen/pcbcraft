        json!({"revision":self.revision
"project":self.project
"history":{"undo":self.undo.len()
"redo":self.redo.len()}
"capabilities":{"schematic":"flat explicit pin nets; embedded multi-unit packages with independent unit placement and one physical pin map"
"pcb":"front SMD and plated through-hole circle/rect/roundrect/oval/chamfered/custom-polygon pads
 circular/oval offset drills; two copper layers
 simple tracks
 rectangular outline and circular cutouts; initial Gerber copper writer (pads and tracks only)"
"library":"native project-library symbol/footprint authoring
 reusable .pcbcraft-lib collections
 embedded placed copies"
"checks":"initial pin assignment and conservative copper bounds with same-pad plated layer bridges and drilled-hole contact tests; not comprehensive ERC/DRC"
"kicad":"native library reading and limited PCB export; no general project import or library writer"
"routing":"orthogonal routes on chosen copper layer; no obstacle avoidance or shove"
"3d":false}})
