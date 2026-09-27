package dev.makepad.octosense.dnd;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Arrays;
import java.util.Locale;

/** No native IDs, condition URIs, policy objects or component names cross this boundary. */
public final class DndSettingsContract {
    private DndSettingsContract() {}
    public static final int PAGE_SIZE=20, MAX_RULES=256;
    public enum Field {
        CALLS("calls",8), MESSAGES("messages",4), CONVERSATIONS("conversations",256),
        REPEAT_CALLERS("repeat_callers",16), ALARMS("alarms",32), MEDIA("media",64),
        SYSTEM("system",128), REMINDERS("reminders",1), EVENTS("events",2);
        public final String wire; public final int bit;
        Field(String wire,int bit) {this.wire=wire;this.bit=bit;}
        public static Field parse(String value) {
            for(Field field:values())if(field.wire.equals(value))return field;
            throw new IllegalArgumentException("Unknown interruption category");
        }
        public String value(String value) {
            if(value==null)throw new IllegalArgumentException("Missing policy choice");
            boolean valid=this==CALLS||this==MESSAGES
                ?Arrays.asList("anyone","contacts","starred","none").contains(value)
                :this==CONVERSATIONS?Arrays.asList("all","important","none").contains(value)
                :value.equals("on")||value.equals("off");
            if(!valid)throw new IllegalArgumentException("Invalid policy choice");return value;
        }
    }
    public static String key(String value) {
        if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid observed DND target");return value;
    }
    public static int offset(int value,String generation) {
        if(value<0||value>=MAX_RULES||value%PAGE_SIZE!=0||value>0&&generation==null)
            throw new IllegalArgumentException("Invalid rule page");
        if(generation!=null)key(generation);return value;
    }
    public static String name(String name) {
        if(name==null||name.trim().isEmpty()||name.codePointCount(0,name.length())>100
            ||name.codePoints().anyMatch(Character::isISOControl))throw new IllegalArgumentException("Invalid schedule name");return name;
    }
    public static int minute(int value) {
        if(value<0||value>1439)throw new IllegalArgumentException("Invalid schedule time");return value;
    }
    public static int[] days(int[] values) {
        if(values==null||values.length>7)throw new IllegalArgumentException("Invalid schedule days");
        int[] copy=values.clone();Arrays.sort(copy);
        for(int i=0;i<copy.length;i++)if(copy[i]<1||copy[i]>7||i>0&&copy[i-1]==copy[i])throw new IllegalArgumentException("Invalid schedule days");return copy;
    }
    public static final class Schedule {
        public final String name; public final int[] days; public final int start,end;
        public final boolean exitAtAlarm,enabled;
        public Schedule(String name,int[] days,int start,int end,boolean exitAtAlarm,boolean enabled) {
            this.name=name(name);this.days=days(days);this.start=minute(start);this.end=minute(end);
            this.exitAtAlarm=exitAtAlarm;this.enabled=enabled;
        }
    }
    /** All six pinned native Policy fields, including unknown bits, survive a single-field edit. */
    public static final class Policy {
        public final int categories,calls,messages,effects,state,conversations;
        public Policy(int categories,int calls,int messages,int effects,int state,int conversations) {
            this.categories=categories;this.calls=calls;this.messages=messages;this.effects=effects;this.state=state;this.conversations=conversations;
        }
        public String value(Field field) {
            if((categories&field.bit)==0)return field==Field.CALLS||field==Field.MESSAGES||field==Field.CONVERSATIONS?"none":"off";
            if(field==Field.CALLS||field==Field.MESSAGES) {
                int sender=field==Field.CALLS?calls:messages;return sender==0?"anyone":sender==1?"contacts":sender==2?"starred":null;
            }
            if(field==Field.CONVERSATIONS)return conversations==1?"all":conversations==2?"important":conversations==3?"none":null;
            return "on";
        }
        public Policy change(Field field,String value) {
            field.value(value);int category=categories,call=calls,message=messages,conversation=conversations;
            if(value.equals("off")||value.equals("none"))category&=~field.bit;else category|=field.bit;
            if(field==Field.CALLS||field==Field.MESSAGES) {
                int sender=value.equals("anyone")?0:value.equals("contacts")?1:value.equals("starred")?2:-1;
                if(sender>=0) {if(field==Field.CALLS)call=sender;else message=sender;}
            }
            if(field==Field.CONVERSATIONS)conversation=value.equals("all")?1:value.equals("important")?2:3;
            return new Policy(category,call,message,effects,state,conversation);
        }
        public String fingerprint() {return hash("policy",String.valueOf(categories),String.valueOf(calls),String.valueOf(messages),String.valueOf(effects),String.valueOf(state),String.valueOf(conversations));}
    }
    public static String hash(String... values) {
        try {
            MessageDigest digest=MessageDigest.getInstance("SHA-256");
            for(String value:values) {byte[] bytes=String.valueOf(value).getBytes(StandardCharsets.UTF_8);digest.update(ByteBuffer.allocate(4).putInt(bytes.length).array());digest.update(bytes);}
            StringBuilder result=new StringBuilder();for(byte b:digest.digest())result.append(String.format(Locale.ROOT,"%02x",b&255));return result.toString();
        }catch(java.security.NoSuchAlgorithmException impossible){throw new IllegalStateException(impossible);}
    }
}
