package dev.makepad.octosense.quickstep;

import android.content.Context;
import android.content.res.ColorStateList;
import android.content.res.Configuration;
import android.graphics.Canvas;
import android.graphics.ColorFilter;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PixelFormat;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.RippleDrawable;
import android.view.Gravity;
import android.widget.Button;
import android.widget.ImageButton;
import android.widget.TextView;

/** Small native design system: adaptive surfaces, real text and vector icons. */
public final class GlobalShadeStyle {
    public final Context context;
    public final boolean dark;
    public final int background, surface, elevated, text, muted, accent, onAccent, outline;
    public GlobalShadeStyle(Context context) {
        this.context = context;
        dark = (context.getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK)
                == Configuration.UI_MODE_NIGHT_YES;
        // The launcher's shade language (mobile_shade.rs): a deep violet
        // ground, cards of #2C2E3E over it, ink #F5F5FA, and the Material
        // purple #6750A4 as the one accent, white on it, in both appearances.
        background = dark ? 0xff181624 : 0xfff3effa;
        surface = dark ? 0xff2c2e3e : 0xffffffff;
        elevated = dark ? 0xff3a3c52 : 0xffe9e3f6;
        text = dark ? 0xfff5f5fa : 0xff1a1a22;
        muted = dark ? 0xffbab6cc : 0xff5c5870;
        accent = 0xff6750a4;
        onAccent = 0xffffffff;
        outline = dark ? 0xff413e58 : 0xffd6cfe8;
    }
    public int dp(float value) { return Math.round(value * context.getResources().getDisplayMetrics().density); }
    public GradientDrawable shape(int color, int radius) {
        GradientDrawable d = new GradientDrawable(); d.setColor(color); d.setCornerRadius(dp(radius)); return d;
    }
    public Drawable touch(int color, int radius) {
        return new RippleDrawable(ColorStateList.valueOf(dark ? 0x28ffffff : 0x246750a4), shape(color, radius), shape(0xffffffff, radius));
    }
    public TextView label(String value, int size, int color) {
        TextView v = new TextView(context); v.setText(value); v.setTextSize(size); v.setTextColor(color);
        v.setFontFeatureSettings("kern"); v.setIncludeFontPadding(false); v.setFallbackLineSpacing(true);
        v.setBreakStrategy(android.graphics.text.LineBreaker.BREAK_STRATEGY_HIGH_QUALITY);
        v.setTextDirection(android.view.View.TEXT_DIRECTION_FIRST_STRONG);
        v.setTextAlignment(android.view.View.TEXT_ALIGNMENT_VIEW_START); return v;
    }
    public void medium(TextView view) { view.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL)); }
    public Button button(String label, Runnable action, boolean primary) {
        Button b = new Button(context); b.setText(label); b.setTextSize(14); b.setAllCaps(false); medium(b);
        b.setTextColor(primary ? onAccent : text); b.setBackground(touch(primary ? accent : elevated, 16));
        b.setMinHeight(dp(48)); b.setMinimumHeight(dp(48)); b.setMinWidth(0); b.setMinimumWidth(0);
        b.setPadding(dp(16), dp(10), dp(16), dp(10)); b.setStateListAnimator(null); b.setGravity(Gravity.CENTER);
        b.setSingleLine(false); b.setMaxLines(3);
        b.setOnClickListener(v -> action.run()); return b;
    }
    public ImageButton iconButton(String name, String description, Runnable action) {
        ImageButton button = new ImageButton(context); button.setImageDrawable(icon(name, text));
        button.setContentDescription(description); button.setBackground(touch(elevated, 24)); button.setPadding(dp(13), dp(13), dp(13), dp(13));
        button.setMinimumWidth(dp(48)); button.setMinimumHeight(dp(48)); button.setOnClickListener(v -> action.run()); return button;
    }
    public Drawable icon(String name, int color) { return new Symbol(name, color, dp(24)); }

    /** A consistent 24-unit stroke family; no fonts, emoji, network assets or raster scaling. */
    private static final class Symbol extends Drawable {
        final String name; final int size; final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        final Path path = new Path();
        Symbol(String name, int color, int size) {
            this.name = name; this.size = size; paint.setColor(color); paint.setStrokeWidth(1.7f);
            paint.setStyle(Paint.Style.STROKE); paint.setStrokeCap(Paint.Cap.ROUND); paint.setStrokeJoin(Paint.Join.ROUND);
        }
        private void line(Canvas c, float... p) { path.reset(); path.moveTo(p[0], p[1]); for (int i = 2; i < p.length; i += 2) path.lineTo(p[i], p[i+1]); c.drawPath(path, paint); }
        private void box(Canvas c, float l, float t, float r, float b, float radius) { c.drawRoundRect(l, t, r, b, radius, radius, paint); }
        private void circle(Canvas c, float x, float y, float r) { c.drawCircle(x, y, r, paint); }
        @Override public void draw(Canvas canvas) {
            canvas.save(); canvas.translate(getBounds().left, getBounds().top);
            canvas.scale(getBounds().width()/24f, getBounds().height()/24f);
            if (getLayoutDirection()==android.view.View.LAYOUT_DIRECTION_RTL && (name.equals("back") || name.equals("chevron"))) {
                canvas.translate(24,0); canvas.scale(-1,1);
            }
            switch (name) {
                case "wifi":
                    canvas.drawArc(new RectF(1, 5, 23, 23), 224, 92, false, paint);
                    canvas.drawArc(new RectF(5, 9, 19, 23), 224, 92, false, paint);
                    canvas.drawArc(new RectF(9, 13, 15, 21), 229, 82, false, paint); circle(canvas,12,19,.7f); break;
                case "bluetooth": line(canvas,7,7,17,17,12,21,12,3,17,7,7,17); break;
                case "torch": line(canvas,5,3,19,3,19,7,15,11,15,21,9,21,9,11,5,7,5,3); line(canvas,5,7,19,7); line(canvas,11,14,13,14); break;
                case "rotation": box(canvas,8,7,16,18,2); line(canvas,3,10,3,6,7,6); line(canvas,21,14,21,18,17,18); break;
                case "moon": path.reset(); path.moveTo(18.7f,16.1f); path.cubicTo(8,21,2.7f,10.3f,10.8f,3); path.cubicTo(8,12,13,16,18.7f,16.1f); canvas.drawPath(path,paint); break;
                case "battery": box(canvas,3,7,19,17,3); line(canvas,22,10,22,14); line(canvas,7,10,7,14); line(canvas,10,10,10,14); break;
                case "sun": circle(canvas,12,12,3.5f); for(int i=0;i<8;i++) { double a=i*Math.PI/4; line(canvas,12+(float)Math.cos(a)*7,12+(float)Math.sin(a)*7,12+(float)Math.cos(a)*9,12+(float)Math.sin(a)*9); } break;
                case "volume": line(canvas,3,9,7,9,12,5,12,19,7,15,3,15,3,9); canvas.drawArc(new RectF(12,6,22,18),-55,110,false,paint); break;
                case "close": line(canvas,7,7,17,17); line(canvas,17,7,7,17); break;
                case "back": line(canvas,14,6,8,12,14,18); break;
                case "chevron": line(canvas,9,6,15,12,9,18); break;
                case "check": line(canvas,5,12,10,17,19,7); break;
                case "bell": path.reset(); path.moveTo(5,17); path.lineTo(7,14); path.lineTo(7,9); path.cubicTo(7,2,17,2,17,9); path.lineTo(17,14); path.lineTo(19,17); path.close(); canvas.drawPath(path,paint); canvas.drawArc(new RectF(9,17,15,22),0,180,false,paint); break;
                case "settings":
                    line(canvas,4,6,20,6); line(canvas,4,12,20,12); line(canvas,4,18,20,18);
                    paint.setStyle(Paint.Style.FILL); circle(canvas,9,6,2.2f); circle(canvas,16,12,2.2f); circle(canvas,8,18,2.2f); paint.setStyle(Paint.Style.STROKE); break;
                case "grid": box(canvas,3,3,10,10,2); box(canvas,14,3,21,10,2); box(canvas,3,14,10,21,2); box(canvas,14,14,21,21,2); break;
                case "shield": line(canvas,12,2,20,6,19,15,12,22,5,15,4,6,12,2); line(canvas,8,11,11,14,16,9); break;
                case "mobile": box(canvas,7,2,17,22,3); line(canvas,11,18,13,18); break;
                case "hotspot": circle(canvas,12,12,2); canvas.drawArc(new RectF(6,6,18,18),-55,110,false,paint); canvas.drawArc(new RectF(6,6,18,18),125,110,false,paint); canvas.drawArc(new RectF(2,2,22,22),-55,110,false,paint); canvas.drawArc(new RectF(2,2,22,22),125,110,false,paint); break;
                case "accessibility": circle(canvas,12,4,2); line(canvas,4,9,20,9); line(canvas,12,9,12,14,7,21); line(canvas,12,14,17,21); break;
                default: circle(canvas,12,12,8); line(canvas,12,8,12,13); circle(canvas,12,16,.5f);
            }
            canvas.restore();
        }
        @Override public int getIntrinsicWidth() { return size; }
        @Override public int getIntrinsicHeight() { return size; }
        @Override public void setAlpha(int alpha) { paint.setAlpha(alpha); invalidateSelf(); }
        @Override public void setColorFilter(ColorFilter filter) { paint.setColorFilter(filter); invalidateSelf(); }
        @Override public int getOpacity() { return PixelFormat.TRANSLUCENT; }
    }
}
