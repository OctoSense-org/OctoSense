package android.app;
public class Activity extends android.content.Context {
 public static final int RESULT_CANCELED=0,RESULT_OK=-1;public boolean finished;public android.content.Intent intent;public android.content.Intent confirmation;public int request;
 public android.view.Window getWindow(){return new android.view.Window();}
 public android.content.Intent getIntent(){return intent;}public void setIntent(android.content.Intent i){intent=i;}
 public void finish(){finished=true;}
 protected void onCreate(android.os.Bundle state){}protected void onNewIntent(android.content.Intent i){}
 protected void onSaveInstanceState(android.os.Bundle state){}protected void onActivityResult(int q,int r,android.content.Intent i){}
 public void startActivityForResult(android.content.Intent i,int r){confirmation=i;request=r;}public void onBackPressed(){finish();}
}
