package dev.makepad.octosense.storagespacefixture;
public final class ManageSpaceActivity extends android.app.Activity {
 @Override public void onCreate(android.os.Bundle saved){super.onCreate(saved);android.widget.LinearLayout layout=new android.widget.LinearLayout(this);layout.setOrientation(1);android.widget.TextView text=new android.widget.TextView(this);text.setText("Synthetic app-owned storage management");layout.addView(text);android.widget.Button back=new android.widget.Button(this);back.setText("Return without changes");back.setOnClickListener(v->finish());layout.addView(back);setContentView(layout);}
}
