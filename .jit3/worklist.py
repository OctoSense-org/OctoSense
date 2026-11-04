import sys,json,collections
rows=[json.loads(l) for l in open(".jit3/check4.log") if l.startswith("{") and '"file"' in l]
fam={"android_integration":"android","accessibility":"accessibility","Accessibility":"accessibility","with_script_vm_id_trusted":"script-vm-trust","Lane":"thread-lane","CloseDecision":"module-host","ModuleCloseAction":"module-host","ModuleWindows":"module-host","WindowRequest":"module-host","splash_host":"splash-host","splash_policy":"splash-policy","set_policy":"splash-policy","PeerLink":"ai-services-links","PeerDown":"ai-services-links","PeerUp":"ai-services-links","UiPort":"ai-services-links","PendingPeerLinks":"ai-services-links","PendingServiceLinks":"ai-services-links","NativeQr":"native-qr","Readback":"readback","ModalBounds":"modal-bounds","TextInputStateQuery":"ime-state-query","terminal::ai":"terminal-ai","json!":"relax-json-macro","include_str":"relax-include-str","screen_origin":"screen-origin"}
out=collections.defaultdict(list)
for r in rows:
    m=r["message"]; f=r["file"].split("/jit3-octosense/")[-1]; k="other"
    for key,v in fam.items():
        if key in m: k=v; break
    out[k].append("%s:%s: %s" % (f, r["line"], m[:140]))
with open(".jit3/port-worklist.md","w") as w:
    w.write("# jit3 port worklist (from check4: bare relax, 182 errors)\n\n")
    for k,v in sorted(out.items(), key=lambda kv:-len(kv[1])):
        w.write("## %s (%d)\n" % (k,len(v)))
        for x in v: w.write("- %s\n" % x)
        w.write("\n")
for k,v in sorted(out.items(), key=lambda kv:-len(kv[1])): print("%4d %s" % (len(v),k))
