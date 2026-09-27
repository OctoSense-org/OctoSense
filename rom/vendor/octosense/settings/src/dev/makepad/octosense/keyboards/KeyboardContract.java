package dev.makepad.octosense.keyboards;

/** Agent84/85 argument contract. No package, component, settings row, or subtype setter. */
public final class KeyboardContract {
    private KeyboardContract(){}
    public static void key(String value){if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid observed keyboard target");}
    public static void read(long request,String query,int offset){
        if(request<=0||query==null||query.codePointCount(0,query.length())>80||query.codePoints().anyMatch(Character::isISOControl)||offset<0||offset>=KeyboardPolicy.MAX_METHODS||offset%KeyboardPolicy.PAGE_SIZE!=0)throw new IllegalArgumentException("Invalid keyboard read");
    }
    public static KeyboardPolicy.Action flow(long request,String observed,String target,String operation){
        if(request<=0||operation==null)throw new IllegalArgumentException("Invalid keyboard operation");key(observed);
        KeyboardPolicy.Action action;
        switch(operation){case "enable":action=KeyboardPolicy.Action.ENABLE;break;case "disable":action=KeyboardPolicy.Action.DISABLE;break;case "provider_settings":action=KeyboardPolicy.Action.SETTINGS;break;case "subtypes":action=KeyboardPolicy.Action.SUBTYPES;break;case "choose_default":action=KeyboardPolicy.Action.PICK_DEFAULT;break;default:throw new IllegalArgumentException("Unknown keyboard operation");}
        if(action==KeyboardPolicy.Action.PICK_DEFAULT){if(!"".equals(target))throw new IllegalArgumentException("Native picker has no caller-chosen target");}else key(target);
        return action;
    }
}
