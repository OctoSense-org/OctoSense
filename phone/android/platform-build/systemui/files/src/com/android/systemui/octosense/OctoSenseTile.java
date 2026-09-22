/* Copyright 2026 OctoSense. Licensed under the Apache License, Version 2.0. */
package com.android.systemui.octosense;

import android.content.Intent;
import android.os.Handler;
import android.os.Looper;
import android.service.quicksettings.Tile;
import android.widget.Button;
import androidx.annotation.Nullable;
import com.android.internal.logging.MetricsLogger;
import com.android.systemui.animation.Expandable;
import com.android.systemui.dagger.qualifiers.Background;
import com.android.systemui.dagger.qualifiers.Main;
import com.android.systemui.plugins.ActivityStarter;
import com.android.systemui.plugins.FalsingManager;
import com.android.systemui.plugins.qs.QSTile;
import com.android.systemui.plugins.statusbar.StatusBarStateController;
import com.android.systemui.qs.QSHost;
import com.android.systemui.qs.QsEventLogger;
import com.android.systemui.qs.logging.QSLogger;
import com.android.systemui.qs.tileimpl.QSTileImpl;
import com.android.systemui.res.R;
import javax.inject.Inject;

/** Native tile: Android owns unlock and launch; no overlay or exported command API. */
public final class OctoSenseTile extends QSTileImpl<QSTile.State> {
    public static final String TILE_SPEC = "octosense";
    @Inject public OctoSenseTile(QSHost host, QsEventLogger events,
            @Background Looper looper, @Main Handler handler, FalsingManager falsing,
            MetricsLogger metrics, StatusBarStateController status,
            ActivityStarter starter, QSLogger logger) {
        super(host, events, looper, handler, falsing, metrics, status, starter, logger);
    }
    @Override public QSTile.State newTileState() {
        QSTile.State state = new QSTile.State();
        state.handlesLongClick = false;
        state.expandedAccessibilityClassName = Button.class.getName();
        return state;
    }
    @Override protected void handleClick(@Nullable Expandable expandable) {
        mActivityStarter.postStartActivityDismissingKeyguard(getLongClickIntent(), 0);
    }
    @Override protected void handleUpdateState(QSTile.State state, Object arg) {
        state.state = Tile.STATE_INACTIVE;
        state.label = getTileLabel();
        state.secondaryLabel = mContext.getString(R.string.octosense_brand);
        state.contentDescription = state.label + ", " + state.secondaryLabel;
        state.icon = maybeLoadResourceIcon(R.drawable.ic_octosense_device);
    }
    @Override public CharSequence getTileLabel() { return mContext.getString(R.string.octosense_device); }
    @Override public int getMetricsCategory() { return 0; }
    @Override public Intent getLongClickIntent() {
        return new Intent(mContext, OctoSenseSystemActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
    }
}
