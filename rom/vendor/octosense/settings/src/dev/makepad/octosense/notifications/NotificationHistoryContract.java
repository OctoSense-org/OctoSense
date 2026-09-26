package dev.makepad.octosense.notifications;

/** Finite, read-only paging contract. No package, listener token or action is accepted. */
public final class NotificationHistoryContract {
    public static final int PAGE_SIZE=20, MAX_ROWS=2000, MAX_SCAN=10000;
    public static final long LIFETIME_MS=120000;
    private NotificationHistoryContract() {}
    public static void request(long id,String key,int offset) {
        if(id<=0||offset<0||offset>=MAX_ROWS||offset%PAGE_SIZE!=0
                ||(key==null&&offset!=0)||key!=null&&!key.matches("[0-9a-f]{64}"))
            throw new IllegalArgumentException("Invalid history page");
    }
    public static String text(String value,int limit) {
        if(value==null) return "";
        StringBuilder result=new StringBuilder();int count=0;
        for(int at=0;at<value.length()&&count<limit;) {
            int point=value.codePointAt(at);at+=Character.charCount(point);
            if(Character.isISOControl(point)&&point!='\n'&&point!='\t') continue;
            if(point>=0xd800&&point<=0xdfff) continue;
            result.appendCodePoint(point);count++;
        }
        return result.toString();
    }
}
