package android.net; public class Uri {final String value;private Uri(String v){value=v;}public static Uri parse(String v){return new Uri(v);}public String toString(){return value;}}
