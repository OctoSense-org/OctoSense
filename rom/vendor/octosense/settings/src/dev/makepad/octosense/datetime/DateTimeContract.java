package dev.makepad.octosense.datetime;

import java.time.DateTimeException;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.time.format.ResolverStyle;
import java.util.List;
import java.util.Locale;

/** Civil time is parsed strictly; missing DST hours must never be silently moved. */
public final class DateTimeContract {
    private DateTimeContract() {}
    public static final DateTimeFormatter CIVIL = DateTimeFormatter.ofPattern("uuuu-MM-dd HH:mm", Locale.ROOT)
            .withResolverStyle(ResolverStyle.STRICT);
    public enum Action {
        CLOCK("clock"), ZONE("zone"), AUTO_TIME("auto_time"), AUTO_ZONE("auto_zone");
        public final String wire;
        Action(String wire) {this.wire=wire;}
        public static Action parse(String value) {
            for(Action action:values()) if(action.wire.equals(value)) return action;
            throw new IllegalArgumentException("Unknown time action");
        }
    }
    public static void key(String value) {
        if(value==null||!value.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid time observation");
    }
    public static ZoneId zone(String value) {
        if(value==null||value.length()>100||!ZoneId.getAvailableZoneIds().contains(value))
            throw new IllegalArgumentException("Unknown time zone");
        return ZoneId.of(value);
    }
    public static LocalDateTime civil(String value,int minimumYear,int maximumYear) {
        if(value==null||!value.matches("[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}"))
            throw new IllegalArgumentException("Use YYYY-MM-DD HH:MM");
        try {
            LocalDateTime local=LocalDateTime.parse(value,CIVIL);
            if(local.getYear()<minimumYear||local.getYear()>maximumYear) throw new IllegalArgumentException("Date outside supported range");
            return local;
        } catch(DateTimeException invalid) {throw new IllegalArgumentException("Invalid calendar date or time");}
    }
    public static long millis(String value,String zone,String occurrence,int minimumYear,int maximumYear) {
        if(!"first".equals(occurrence)&&!"second".equals(occurrence)) throw new IllegalArgumentException("Invalid repeated-hour choice");
        LocalDateTime local=civil(value,minimumYear,maximumYear);
        List<ZoneOffset> offsets=zone(zone).getRules().getValidOffsets(local);
        if(offsets.isEmpty()) throw new IllegalArgumentException("This local time does not exist because the clock moves forward");
        return local.toInstant(offsets.get("second".equals(occurrence)?offsets.size()-1:0)).toEpochMilli();
    }
    public static boolean enabled(String value) {
        if("on".equals(value)) return true;
        if("off".equals(value)) return false;
        throw new IllegalArgumentException("Invalid time switch");
    }
    public static void validate(Action action,String value,String occurrence) {
        switch(action) {
            case CLOCK: civil(value,1970,9999);if(!"first".equals(occurrence)&&!"second".equals(occurrence)) throw new IllegalArgumentException("Invalid repeated-hour choice");break;
            case ZONE: zone(value);if(occurrence!=null) throw new IllegalArgumentException("Unexpected occurrence");break;
            default: enabled(value);if(occurrence!=null) throw new IllegalArgumentException("Unexpected occurrence");
        }
    }
}
