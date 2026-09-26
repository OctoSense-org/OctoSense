import dev.makepad.octosense.controls.SoundFeedbackContract;
import dev.makepad.octosense.controls.SoundFeedbackContract.*;
import java.util.Arrays;
import java.util.EnumMap;
public final class SoundFeedbackContractTest {
 static void check(boolean ok){if(!ok)throw new AssertionError();}
 static final class Memory implements Store {
  boolean authority=true,confirm=true;Slot fail,retire;int writes;EnumMap<Slot,Integer> state=new EnumMap<>(Slot.class);
  public boolean allowed(){return authority;}
  public boolean write(Slot slot,int value){writes++;if(slot==fail)return false;if(confirm)state.put(slot,value);if(slot==retire)authority=false;return true;}
  public Integer read(Slot slot){return state.get(slot);}
 }
 public static void main(String[] args){
  check(Arrays.equals(SoundFeedbackContract.choices(1,2),new String[]{"off","default"}));
  check(Arrays.equals(SoundFeedbackContract.choices(2,2),new String[]{"off","low","high"}));
  check(Arrays.equals(SoundFeedbackContract.choices(3,2),new String[]{"off","low","medium","high"}));
  check(SoundFeedbackContract.choices(0,2).length==0);check(SoundFeedbackContract.choices(1,-1).length==0);
  check(SoundFeedbackContract.intensity(null,2,1).equals("default"));check(SoundFeedbackContract.intensity("3",2,1).equals("high"));check(SoundFeedbackContract.intensity("0",0,1).equals("off"));
  check(SoundFeedbackContract.intensity(null,2,2).equals("medium"));check(SoundFeedbackContract.intensity(null,2,3).equals("medium"));
  check(SoundFeedbackContract.intensity("3",3,2).equals("high"));check(SoundFeedbackContract.intensity("1",1,3).equals("low"));
  check(SoundFeedbackContract.intensity(null,2,0)==null);
  for(String raw:new String[]{"-1","4"," 1","1.0","1\n","9999999999"})check(SoundFeedbackContract.intensity(raw,2,1)==null);
  check(SoundFeedbackContract.toggle(null).equals("on"));check(SoundFeedbackContract.toggle("0").equals("off"));check(SoundFeedbackContract.toggle("3")==null);
  check(SoundFeedbackContract.stored("default",1,2)==2);
  try{SoundFeedbackContract.stored("high",1,2);throw new AssertionError();}catch(IllegalArgumentException expected){}
  Memory m=new Memory();check(SoundFeedbackContract.apply(Coupling.RING,2,2,m).equals("control_applied"));check(m.state.get(Slot.RING_LEGACY)==1&&m.writes==2);
  m=new Memory();check(SoundFeedbackContract.apply(Coupling.TOUCH,0,2,m).equals("control_applied"));check(m.state.get(Slot.TOUCH_LEGACY)==0&&m.state.get(Slot.HARDWARE_TOUCH)==2&&m.writes==3);
  m=new Memory();m.fail=Slot.RING_LEGACY;check(SoundFeedbackContract.apply(Coupling.RING,2,2,m).equals("control_partial"));check(m.state.get(Slot.PRIMARY)==2&&m.writes==2);
  m=new Memory();m.fail=Slot.HARDWARE_TOUCH;check(SoundFeedbackContract.apply(Coupling.TOUCH,3,2,m).equals("control_partial"));check(m.state.get(Slot.PRIMARY)==3&&m.writes==3);
  m=new Memory();m.retire=Slot.PRIMARY;check(SoundFeedbackContract.apply(Coupling.TOUCH,3,2,m).equals("control_partial"));check(m.writes==1);
  m=new Memory();m.authority=false;check(SoundFeedbackContract.apply(Coupling.NONE,1,2,m).equals("control_unavailable"));check(m.writes==0);
  m=new Memory();m.confirm=false;check(SoundFeedbackContract.apply(Coupling.NONE,1,2,m).equals("control_requested"));
 }
}
