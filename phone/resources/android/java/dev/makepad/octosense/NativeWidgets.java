package dev.makepad.octosense;

import android.app.Activity;
import android.app.AlertDialog;
import android.appwidget.AppWidgetHost;
import android.appwidget.AppWidgetHostView;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProviderInfo;
import android.content.ComponentName;
import android.content.Intent;
import android.content.pm.LauncherApps;
import android.graphics.Rect;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.UserHandle;
import android.os.UserManager;
import android.util.SizeF;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.TextView;
import dev.makepad.android.MakepadActivity;
import java.io.File;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import org.json.JSONArray;
import org.json.JSONObject;

/** Native widget workspace in the Home activity. Android owns RemoteViews,
 * provider PendingIntents and input. Storage and discovery share Home's bounded
 * worker; only view construction and consent activities run on the main thread. */
public final class NativeWidgets {
    public static final int HOST_ID=0x4f4354;
    private static final int REQUEST_BIND=0x6b10,REQUEST_CONFIGURE=0x6b11,REQUEST_RECONFIGURE=0x6b12;
    interface Dispatch {boolean offer(Runnable task);}
    interface Failure {void report(String reason);}
    interface Publisher {void publish(JSONObject model);}
    private interface Work {void run() throws Exception;}
    private final MakepadActivity activity;
    private final Handler main=new Handler(Looper.getMainLooper());
    private final Dispatch worker;
    private final Failure failure;
    private final Publisher publisher;
    private final String epoch=java.util.UUID.randomUUID().toString();
    private long revision;
    private final AppWidgetManager manager;
    private final AppWidgetHost host;
    private final UserManager users;
    private final WidgetPlacements store;
    // Worker-owned journal. Listener lifecycle is serialized on the main thread.
    private WidgetPlacements.State state;
    private boolean listening;
    // Main-thread view state; lifecycle flags also gate worker publications.
    private volatile boolean destroyed,shown,resumed;
    private java.util.function.Consumer<Boolean> visibility=value -> {};
    public void setVisibilityListener(java.util.function.Consumer<Boolean> listener) {visibility=listener;}
    private LinearLayout workspace,rows;
    private FrameLayout pageRoot;
    private final HashMap<Integer,View> pageViews=new HashMap<>();
    private final HashMap<Integer,WidgetPlacements.Entry> pageEntries=new HashMap<>();
    private final HashMap<Integer,AppWidgetProviderInfo> pageProviders=new HashMap<>();
    private long pageRevision;
    private JSONArray pageLayout=new JSONArray();
    private long layoutRevision;
    private final HashMap<Integer,AppWidgetHostView> views=new HashMap<>();
    private final HashMap<Integer,String> viewProviders=new HashMap<>();
    private final java.util.concurrent.atomic.AtomicLong presentation=new java.util.concurrent.atomic.AtomicLong();

    public NativeWidgets(MakepadActivity activity,Dispatch worker,Failure failure,Publisher publisher) {
        this(activity,worker,failure,publisher,HOST_ID,new File(activity.getFilesDir(),"launcher-widgets.json"));
    }
    NativeWidgets(MakepadActivity activity,Dispatch worker,Failure failure,Publisher publisher,int hostId,File journal) {
        this.activity=activity;this.worker=worker;this.failure=failure;this.publisher=publisher;
        manager=AppWidgetManager.getInstance(activity);users=activity.getSystemService(UserManager.class);
        store=new WidgetPlacements(journal);
        host=new AppWidgetHost(activity,hostId) {
            @Override protected void onProvidersChanged() {refresh();}
            @Override protected void onProviderChanged(int id,AppWidgetProviderInfo info) {
                super.onProviderChanged(id,info);refresh();
            }
            @Override public void onAppWidgetRemoved(int id) {refresh();}
        };
    }
    public JSONObject validationState() throws org.json.JSONException {
        JSONArray placements=new JSONArray();
        for(java.util.Map.Entry<Integer,View> entry:pageViews.entrySet()) {
            View view=entry.getValue();
            placements.put(new JSONObject().put("id",entry.getKey()).put("visible",view.isShown())
                    .put("x",view.getTranslationX()).put("y",view.getTranslationY()).put("width",view.getWidth()).put("height",view.getHeight()));
        }
        return new JSONObject().put("workspace",shown).put("native_views",views.size()).put("model_revision",pageRevision)
                .put("layout_revision",layoutRevision).put("pages_visible",pageRoot!=null&&pageRoot.isShown()).put("pages",placements);
    }
    private void submit(Work work) {
        if(destroyed) return;
        if(!worker.offer(() -> {
            if(destroyed) return;
            try {work.run();}
            catch(Exception e) {
                state=null;
                failure.report("widget_operation_failed_"+e.getClass().getSimpleName());
                main.post(() -> {if(shown&&!destroyed) message("Widget operation unavailable. Your stored widgets were kept.");});
            }
        })) main.post(() -> {if(shown&&!destroyed) message("Home is busy. Try the widget operation again.");});
    }
    private WidgetPlacements.State load() throws Exception {
        if(state==null) {
            WidgetPlacements.State loaded=store.read();
            // Never clean allocations when storage cannot be decoded.
            for(int id:host.getAppWidgetIds()) if(!loaded.owns(id)) host.deleteAppWidgetId(id);
            state=loaded;
        }
        return state;
    }
    private void save(WidgetPlacements.State next) throws Exception {store.write(next);state=next;}
    private WidgetPlacements.Entry find(int id) throws Exception {
        for(WidgetPlacements.Entry entry:load().entries) if(entry.id==id) return entry;
        throw new IllegalArgumentException("Widget is no longer placed");
    }
    private AppWidgetProviderInfo info(WidgetPlacements.Entry entry) {
        UserHandle profile=users.getUserForSerialNumber(entry.user);
        if(profile==null || !users.isUserUnlocked(profile) || users.isQuietModeEnabled(profile)) return null;
        AppWidgetProviderInfo info=manager.getAppWidgetInfo(entry.id);
        return info!=null && info.provider.flattenToString().equals(entry.provider)
                && users.getSerialNumberForUser(info.getProfile())==entry.user ? info:null;
    }
    private Bundle options(WidgetPlacements.Entry entry) {
        Rect padding=AppWidgetHostView.getDefaultPaddingForWidget(activity,ComponentName.unflattenFromString(entry.provider),null);
        float density=activity.getResources().getDisplayMetrics().density;
        int width=Math.max(1,entry.width-Math.round((padding.left+padding.right)/density));
        int height=Math.max(1,entry.height-Math.round((padding.top+padding.bottom)/density));
        Bundle options=new Bundle();
        options.putInt(AppWidgetManager.OPTION_APPWIDGET_HOST_CATEGORY,AppWidgetProviderInfo.WIDGET_CATEGORY_HOME_SCREEN);
        options.putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH,width);options.putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH,width);
        options.putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT,height);options.putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT,height);
        if(Build.VERSION.SDK_INT>=31) {ArrayList<SizeF> sizes=new ArrayList<>();sizes.add(new SizeF(width,height));options.putParcelableArrayList(AppWidgetManager.OPTION_APPWIDGET_SIZES,sizes);}
        return options;
    }
    public void show() {
        if(destroyed) return;
        if(!shown) visibility.accept(true);
        shown=true;
        pageViews.clear();
        if(pageRoot!=null) {pageRoot.setVisibility(View.GONE);pageRoot.removeAllViews();}
        if(workspace==null) {
            workspace=new LinearLayout(activity);workspace.setOrientation(LinearLayout.VERTICAL);
            workspace.setBackgroundColor(0xff19171f);workspace.setClickable(true);workspace.setFocusable(true);
            int inset=dp(16);workspace.setPadding(inset,inset,inset,inset);
            workspace.setOnApplyWindowInsetsListener((view,insets) -> {
                view.setPadding(inset+insets.getSystemWindowInsetLeft(),inset+insets.getSystemWindowInsetTop(),
                        inset+insets.getSystemWindowInsetRight(),inset+insets.getSystemWindowInsetBottom());return insets;
            });
            LinearLayout header=line();header.addView(text("Widgets",22),new LinearLayout.LayoutParams(0,-2,1));
            header.addView(button("Add",v -> choose()));header.addView(button("Done",v -> hide()));workspace.addView(header);
            ScrollView scroll=new ScrollView(activity);rows=new LinearLayout(activity);rows.setOrientation(LinearLayout.VERTICAL);scroll.addView(rows);
            workspace.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
            activity.getApplicationOverlay().addView(workspace,new FrameLayout.LayoutParams(-1,-1));workspace.requestApplyInsets();
        }
        workspace.setVisibility(resumed?View.VISIBLE:View.GONE);
        refresh();
    }
    public boolean hide() {
        if(!shown) return false;shown=false;
        visibility.accept(false);
        if(workspace!=null) workspace.setVisibility(View.GONE);
        applyPageLayout();updateListening();return true;
    }
    public void onResume() {resumed=true;refresh();if(shown) show();}
    public void onPause() {
        resumed=false;if(workspace!=null) workspace.setVisibility(View.GONE);
        if(pageRoot!=null) pageRoot.setVisibility(View.GONE);
        submit(this::updateListening);
    }
    public void onDestroy() {
        if(shown) visibility.accept(false);
        destroyed=true;shown=false;resumed=false;main.removeCallbacksAndMessages(null);
        // Stop callbacks for this host. Never delete persistent IDs on activity death.
        try {host.stopListening();} catch(RuntimeException ignored) {}
        if(workspace!=null) activity.getApplicationOverlay().removeView(workspace);
        if(pageRoot!=null) activity.getApplicationOverlay().removeView(pageRoot);
        views.clear();viewProviders.clear();
    }
    private void updateListening() {
        main.post(() -> {
            if(destroyed) return;
            boolean wanted=resumed&&(shown||!pageEntries.isEmpty());
            try {
                if(wanted&&!listening) {host.startListening();listening=true;}
                else if(!wanted&&listening) {host.stopListening();listening=false;}
            } catch(RuntimeException e) {listening=false;message("Android's widget service is unavailable.");}
        });
    }
    public void refresh() {
        if(destroyed) return;
        long generation=presentation.incrementAndGet();
        // Discard the old profile's visible content before asynchronous access
        // checks. A later invalidation also prevents a stale query from showing it.
        main.post(() -> {
            if(destroyed) return;
            if(rows!=null) rows.setVisibility(View.INVISIBLE);
            if(pageRoot!=null) pageRoot.setVisibility(View.GONE);
        });
        submit(() -> {
            WidgetPlacements.State snapshot=load();updateListening();
            ArrayList<AppWidgetProviderInfo> providers=new ArrayList<>();ArrayList<String> labels=new ArrayList<>();
            for(WidgetPlacements.Entry entry:snapshot.entries) {
                AppWidgetProviderInfo provider=info(entry);providers.add(provider);
                labels.add(provider==null?"Unavailable widget":provider.loadLabel(activity.getPackageManager()));
                if(provider!=null) {
                    Bundle next=options(entry),previous=manager.getAppWidgetOptions(entry.id);
                    if(previous.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)!=next.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
                            ||previous.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT)!=next.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT))
                        manager.updateAppWidgetOptions(entry.id,next);
                }
            }
            long modelRevision=++revision;
            JSONArray widgetModels=new JSONArray();
            for(int index=0;index<snapshot.entries.size();index++) {
                WidgetPlacements.Entry entry=snapshot.entries.get(index);
                widgetModels.put(new JSONObject().put("id",entry.id).put("label",labels.get(index)).put("available",providers.get(index)!=null));
            }
            publisher.publish(new JSONObject().put("epoch",epoch).put("revision",modelRevision).put("widgets",widgetModels));
            main.post(() -> {
                if(!destroyed&&presentation.get()==generation) {
                    pageRevision=modelRevision;pageEntries.clear();pageProviders.clear();
                    pageViews.clear();if(pageRoot!=null) pageRoot.removeAllViews();
                    for(int index=0;index<snapshot.entries.size();index++) {
                        WidgetPlacements.Entry entry=snapshot.entries.get(index);pageEntries.put(entry.id,entry);
                        if(providers.get(index)!=null) pageProviders.put(entry.id,providers.get(index));
                    }
                    if(shown) {render(snapshot,providers,labels);rows.setVisibility(View.VISIBLE);}
                    else preparePages();
                    updateListening();applyPageLayout();
                }
            });
        });
    }
    /** Called only from the activity thread; immutable state arrived earlier. */
    public void layout(String payload) {
        if(destroyed||payload.length()>8192) return;
        try {
            JSONObject value=new JSONObject(payload);
            if(!epoch.equals(value.getString("epoch"))) {if(pageRoot!=null) pageRoot.setVisibility(View.GONE);return;}
            JSONArray entries=value.getJSONArray("widgets");if(entries.length()>16) return;
            HashSet<Integer> seen=new HashSet<>();
            for(int index=0;index<entries.length();index++) {
                JSONObject entry=entries.getJSONObject(index);
                if(!seen.add(entry.getInt("id"))) return;
                for(String key:new String[]{"x","y","width","height"}) {
                    double n=entry.getDouble(key);if(!Double.isFinite(n)||Math.abs(n)>2.0) return;
                    if((key.equals("width")||key.equals("height"))&&n<=0.0) return;
                }
            }
            pageLayout=entries;layoutRevision=value.getLong("revision");applyPageLayout();
        } catch(Exception ignored) {if(pageRoot!=null) pageRoot.setVisibility(View.GONE);}
    }
    private void preparePages() {
        if(shown||destroyed) return;
        if(pageRoot==null) {
            pageRoot=new FrameLayout(activity);pageRoot.setClipChildren(true);
            activity.getApplicationOverlay().addView(pageRoot,0,new FrameLayout.LayoutParams(-1,-1));
            pageRoot.addOnLayoutChangeListener((v,l,t,r,b,ol,ot,or,ob) -> {if(r-l!=or-ol||b-t!=ob-ot) applyPageLayout();});
        }
        for(WidgetPlacements.Entry entry:pageEntries.values()) {
            if(pageViews.containsKey(entry.id)||!pageProviders.containsKey(entry.id)) continue;
            try {
                AppWidgetHostView view=views.get(entry.id);
                if(view==null) {
                    view=host.createView(activity,entry.id,pageProviders.get(entry.id));views.put(entry.id,view);viewProviders.put(entry.id,entry.provider);
                }
                if(view.getParent()!=null) ((ViewGroup)view.getParent()).removeView(view);
                HorizontalScrollView horizontal=new HorizontalScrollView(activity);
                horizontal.addView(view,new FrameLayout.LayoutParams(dp(entry.width),dp(entry.height)));
                ScrollView vertical=new ScrollView(activity);vertical.addView(horizontal);vertical.setVisibility(View.GONE);
                pageRoot.addView(vertical);pageViews.put(entry.id,vertical);
            } catch(RuntimeException e) {message("An Android widget could not create its Home view.");}
        }
        views.keySet().retainAll(pageProviders.keySet());viewProviders.keySet().retainAll(pageProviders.keySet());
    }
    private void applyPageLayout() {
        if(destroyed) return;
        if(!shown) preparePages();
        if(pageRoot==null) return;
        boolean visible=resumed&&!shown&&layoutRevision==pageRevision&&pageLayout.length()>0;
        pageRoot.setVisibility(visible?View.VISIBLE:View.GONE);
        if(!visible||pageRoot.getWidth()<=0||pageRoot.getHeight()<=0) return;
        try {
            for(java.util.Map.Entry<Integer,View> placed:pageViews.entrySet()) {
                boolean present=false;
                for(int index=0;index<pageLayout.length();index++) if(pageLayout.getJSONObject(index).getInt("id")==placed.getKey()) {present=true;break;}
                if(!present) placed.getValue().setVisibility(View.GONE);
            }
            for(int index=0;index<pageLayout.length();index++) {
                JSONObject item=pageLayout.getJSONObject(index);View view=pageViews.get(item.getInt("id"));if(view==null) continue;
                int width=Math.max(1,(int)Math.round(item.getDouble("width")*pageRoot.getWidth()));
                int height=Math.max(1,(int)Math.round(item.getDouble("height")*pageRoot.getHeight()));
                WidgetPlacements.Entry placement=pageEntries.get(item.getInt("id"));if(placement==null) continue;
                int availableWidth=width;
                // Empty space below a small widget belongs to the Home pager.
                // Only overflowing widget content needs a scrolling viewport.
                width=Math.min(width,dp(placement.width));height=Math.min(height,dp(placement.height));
                FrameLayout.LayoutParams size=(FrameLayout.LayoutParams)view.getLayoutParams();
                if(size.width!=width||size.height!=height) {size.width=width;size.height=height;view.setLayoutParams(size);}
                view.setTranslationX((float)(item.getDouble("x")*pageRoot.getWidth()+(availableWidth-width)*0.5));
                view.setTranslationY((float)(item.getDouble("y")*pageRoot.getHeight()));view.setVisibility(View.VISIBLE);
            }
        } catch(Exception e) {pageRoot.setVisibility(View.GONE);}
    }
    private void render(WidgetPlacements.State snapshot,ArrayList<AppWidgetProviderInfo> providers,ArrayList<String> labels) {
        rows.removeAllViews();HashSet<Integer> retained=new HashSet<>();
        if(snapshot.pending!=null) {
            rows.addView(text("Finish adding your widget",16));LinearLayout actions=line();
            actions.addView(button("Continue",v -> submit(() -> continuePending())));
            actions.addView(button("Cancel",v -> submit(() -> cancelPending())));rows.addView(actions);
        }
        if(snapshot.entries.isEmpty()) rows.addView(text("Add an Android widget to Home.",16));
        for(int index=0;index<snapshot.entries.size();index++) {
            WidgetPlacements.Entry entry=snapshot.entries.get(index);AppWidgetProviderInfo provider=providers.get(index);
            LinearLayout title=line();title.addView(text(labels.get(index),16),new LinearLayout.LayoutParams(0,-2,1));
            if(provider!=null) {
                if(provider.configure!=null&&Build.VERSION.SDK_INT>=28&&(provider.widgetFeatures&AppWidgetProviderInfo.WIDGET_FEATURE_RECONFIGURABLE)!=0)
                    title.addView(button("Configure",v -> configure(entry.id,REQUEST_RECONFIGURE)));
                if(provider.resizeMode!=AppWidgetProviderInfo.RESIZE_NONE) title.addView(button("Resize",v -> resize(entry,provider)));
            }
            title.addView(button("Remove",v -> new AlertDialog.Builder(activity).setMessage("Remove this widget from Home?")
                    .setNegativeButton("Cancel",null).setPositiveButton("Remove",(d,w) -> submit(() -> remove(entry.id))).show()));rows.addView(title);
            if(provider==null) {rows.addView(text("The provider or profile is unavailable. Unlock the profile to show its content.",14));continue;}
            try {
                AppWidgetHostView view=views.get(entry.id);
                if(view==null||!entry.provider.equals(viewProviders.get(entry.id))) {
                    view=host.createView(activity,entry.id,provider);views.put(entry.id,view);viewProviders.put(entry.id,entry.provider);
                }
                if(view.getParent()!=null) ((ViewGroup)view.getParent()).removeView(view);
                HorizontalScrollView container=new HorizontalScrollView(activity);container.setFillViewport(false);
                container.addView(view,new FrameLayout.LayoutParams(dp(entry.width),dp(entry.height)));rows.addView(container);
                retained.add(entry.id);
            } catch(RuntimeException e) {rows.addView(text("This widget could not create its view. Remove and add it again to retry.",14));}
        }
        views.keySet().retainAll(retained);viewProviders.keySet().retainAll(retained);
    }
    private void choose() {
        submit(() -> {
            if(load().pending!=null) {main.post(() -> message("Finish or cancel the pending widget first."));return;}
            if(state.entries.size()>=WidgetPlacements.MAX_WIDGETS) {main.post(() -> message("Home supports up to 16 widgets."));return;}
            ArrayList<AppWidgetProviderInfo> providers=new ArrayList<>();ArrayList<String> labels=new ArrayList<>();
            for(UserHandle profile:activity.getSystemService(LauncherApps.class).getProfiles()) {
                if(!users.isUserUnlocked(profile)||users.isQuietModeEnabled(profile)) continue;
                for(AppWidgetProviderInfo provider:manager.getInstalledProvidersForProfile(profile)) {
                    if((provider.widgetCategory&AppWidgetProviderInfo.WIDGET_CATEGORY_HOME_SCREEN)==0
                            ||(Build.VERSION.SDK_INT>=28&&(provider.widgetFeatures&AppWidgetProviderInfo.WIDGET_FEATURE_HIDE_FROM_PICKER)!=0)) continue;
                    if(providers.size()>=512) break;
                    providers.add(provider);
                    labels.add(activity.getPackageManager().getUserBadgedLabel(provider.loadLabel(activity.getPackageManager()),profile).toString());
                }
            }
            main.post(() -> {
                if(!shown||destroyed) return;
                if(providers.isEmpty()) {message("No widgets are available in unlocked profiles.");return;}
                new AlertDialog.Builder(activity).setTitle("Add widget").setItems(labels.toArray(new String[0]),
                        (dialog,index) -> submit(() -> begin(providers.get(index)))).setNegativeButton("Cancel",null).show();
            });
        });
    }
    private void begin(AppWidgetProviderInfo provider) throws Exception {
        WidgetPlacements.State current=load();if(current.pending!=null) throw new IllegalStateException("Pending widget exists");
        if(current.entries.size()>=WidgetPlacements.MAX_WIDGETS) throw new IllegalArgumentException("Widget limit reached");
        float density=activity.getResources().getDisplayMetrics().density;
        int width=Math.max(1,Math.round(provider.minWidth/density)),height=Math.max(1,Math.round(provider.minHeight/density));
        if(Build.VERSION.SDK_INT>=31) {
            width=Math.max(width,Math.min(4096,provider.targetCellWidth*72));
            height=Math.max(height,Math.min(4096,provider.targetCellHeight*72));
        }
        Rect padding=AppWidgetHostView.getDefaultPaddingForWidget(activity,provider.provider,null);
        width+=Math.round((padding.left+padding.right)/density);height+=Math.round((padding.top+padding.bottom)/density);
        int id=host.allocateAppWidgetId();
        try {
            WidgetPlacements.Entry pending=new WidgetPlacements.Entry(id,provider.provider.flattenToString(),users.getSerialNumberForUser(provider.getProfile()),width,height);
            save(new WidgetPlacements.State(current.entries,pending,"bind"));
        } catch(Exception e) {host.deleteAppWidgetId(id);throw e;}
        continuePending();
    }
    private void continuePending() throws Exception {
        WidgetPlacements.State current=load();WidgetPlacements.Entry pending=current.pending;if(pending==null) return;
        if(info(pending)==null) {
            UserHandle profile=users.getUserForSerialNumber(pending.user);
            if(profile==null||!users.isUserUnlocked(profile)||users.isQuietModeEnabled(profile)) {
                main.post(() -> message("Unlock the widget's profile, or cancel this addition."));return;
            }
            if(!manager.bindAppWidgetIdIfAllowed(pending.id,profile,ComponentName.unflattenFromString(pending.provider),options(pending))) {
                Intent intent=new Intent(AppWidgetManager.ACTION_APPWIDGET_BIND)
                        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID,pending.id)
                        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_PROVIDER,ComponentName.unflattenFromString(pending.provider))
                        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_PROVIDER_PROFILE,profile)
                        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_OPTIONS,options(pending));
                main.post(() -> {if(!destroyed) try {activity.startActivityForResult(intent,REQUEST_BIND);}
                    catch(RuntimeException e) {message("Android's widget consent screen is unavailable. The pending widget can be cancelled.");}});
                return;
            }
        }
        AppWidgetProviderInfo provider=info(pending);
        if(provider==null) throw new IllegalStateException("Widget binding unavailable");
        if(provider.configure!=null) {
            save(new WidgetPlacements.State(current.entries,pending,"configure"));
            main.post(() -> configure(pending.id,REQUEST_CONFIGURE));
        } else finishPending();
    }
    private void configure(int id,int requestCode) {
        if(destroyed) return;
        try {host.startAppWidgetConfigureActivityForResult(activity,id,0,requestCode,null);}
        catch(RuntimeException e) {
            message("This widget's configuration screen is unavailable. Retry or cancel the pending addition.");
            refresh();
        }
    }
    public boolean onActivityResult(int request,int result,Intent data) {
        if(request!=REQUEST_BIND&&request!=REQUEST_CONFIGURE&&request!=REQUEST_RECONFIGURE) return false;
        if(request==REQUEST_RECONFIGURE) {refresh();return true;}
        submit(() -> {
            WidgetPlacements.State current=load();if(current.pending==null) return;
            int returned=data==null?current.pending.id:data.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID,current.pending.id);
            if(returned!=current.pending.id) throw new IllegalArgumentException("Unexpected widget result ID");
            if(result!=Activity.RESULT_OK) {cancelPending();return;}
            if(request==REQUEST_CONFIGURE) {
                if(!current.stage.equals("configure")) throw new IllegalStateException("Unexpected configuration result");
                finishPending();
            } else continuePending();
        });return true;
    }
    private void finishPending() throws Exception {
        WidgetPlacements.State current=load();if(current.pending==null) return;
        if(info(current.pending)==null) throw new IllegalStateException("Widget provider unavailable");
        ArrayList<WidgetPlacements.Entry> entries=new ArrayList<>(current.entries);entries.add(current.pending);
        save(new WidgetPlacements.State(entries,null,""));refresh();
    }
    private void cancelPending() throws Exception {
        WidgetPlacements.State current=load();if(current.pending==null) return;int id=current.pending.id;
        save(new WidgetPlacements.State(current.entries,null,""));host.deleteAppWidgetId(id);refresh();
    }
    private void remove(int id) throws Exception {
        WidgetPlacements.State current=load();find(id);
        ArrayList<WidgetPlacements.Entry> entries=new ArrayList<>(current.entries);entries.removeIf(entry -> entry.id==id);
        save(new WidgetPlacements.State(entries,current.pending,current.stage));host.deleteAppWidgetId(id);refresh();
    }
    private void resize(WidgetPlacements.Entry entry,AppWidgetProviderInfo provider) {
        LinearLayout fields=new LinearLayout(activity);fields.setOrientation(LinearLayout.VERTICAL);
        float density=activity.getResources().getDisplayMetrics().density;
        Rect padding=AppWidgetHostView.getDefaultPaddingForWidget(activity,provider.provider,null);
        int padX=Math.round((padding.left+padding.right)/density),padY=Math.round((padding.top+padding.bottom)/density);
        int minW=Math.max(1,Math.round(provider.minResizeWidth/density))+padX,minH=Math.max(1,Math.round(provider.minResizeHeight/density))+padY;
        if(provider.minResizeWidth<=0) minW=Math.max(1,Math.round(provider.minWidth/density))+padX;
        if(provider.minResizeHeight<=0) minH=Math.max(1,Math.round(provider.minHeight/density))+padY;
        int maxW=Build.VERSION.SDK_INT>=31&&provider.maxResizeWidth>0?Math.round(provider.maxResizeWidth/density)+padX:Math.max(720,entry.width);
        int maxH=Build.VERSION.SDK_INT>=31&&provider.maxResizeHeight>0?Math.round(provider.maxResizeHeight/density)+padY:Math.max(720,entry.height);
        SeekBar width=dimension(fields,"Width",entry.width,minW,Math.max(minW,Math.min(4096,maxW)),(provider.resizeMode&AppWidgetProviderInfo.RESIZE_HORIZONTAL)!=0);
        SeekBar height=dimension(fields,"Height",entry.height,minH,Math.max(minH,Math.min(4096,maxH)),(provider.resizeMode&AppWidgetProviderInfo.RESIZE_VERTICAL)!=0);
        new AlertDialog.Builder(activity).setTitle("Resize widget").setView(fields).setNegativeButton("Cancel",null)
                .setPositiveButton("Apply",(dialog,which) -> {
                    int w=width.isEnabled()?width.getProgress():entry.width,h=height.isEnabled()?height.getProgress():entry.height;
                    submit(() -> {
                        WidgetPlacements.State current=load();WidgetPlacements.Entry old=find(entry.id);
                        ArrayList<WidgetPlacements.Entry> entries=new ArrayList<>(current.entries);
                        entries.set(entries.indexOf(old),new WidgetPlacements.Entry(old.id,old.provider,old.user,w,h));
                        save(new WidgetPlacements.State(entries,current.pending,current.stage));refresh();
                    });
                }).show();
    }
    private SeekBar dimension(LinearLayout parent,String label,int value,int min,int max,boolean enabled) {
        TextView title=text(label+": "+value+" dp",14);parent.addView(title);
        SeekBar seek=new SeekBar(activity);seek.setMin(min);seek.setMax(max);seek.setProgress(value);seek.setEnabled(enabled);
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            public void onProgressChanged(SeekBar bar,int n,boolean user) {title.setText(label+": "+n+" dp");}
            public void onStartTrackingTouch(SeekBar bar) {} public void onStopTrackingTouch(SeekBar bar) {}
        });parent.addView(seek);return seek;
    }
    private int dp(int value) {return Math.round(value*activity.getResources().getDisplayMetrics().density);}
    private LinearLayout line() {LinearLayout row=new LinearLayout(activity);row.setGravity(Gravity.CENTER_VERTICAL);return row;}
    private TextView text(String value,int size) {TextView view=new TextView(activity);view.setText(value);view.setTextColor(0xffeeeeee);view.setTextSize(size);view.setPadding(0,dp(8),0,dp(8));return view;}
    private Button button(String label,View.OnClickListener click) {Button button=new Button(activity);button.setText(label);button.setOnClickListener(click);return button;}
    private void message(String text) {if(!destroyed&&!activity.isFinishing()) new AlertDialog.Builder(activity).setMessage(text).setPositiveButton("OK",null).show();}
}
