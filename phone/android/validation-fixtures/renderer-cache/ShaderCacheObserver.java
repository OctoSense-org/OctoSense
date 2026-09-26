package dev.makepad.octosense.renderfixture;
import android.opengl.*;
import java.nio.*;
import org.json.JSONObject;
/** Independent public GLES API cache roundtrip. No settings or application data access. */
public final class ShaderCacheObserver {
 static int value(int program,int key){int[] v={0};GLES30.glGetProgramiv(program,key,v,0);return v[0];}
 static int shader(int kind,String source){int s=GLES30.glCreateShader(kind);GLES30.glShaderSource(s,source);GLES30.glCompileShader(s);return s;}
 public static void main(String[] args)throws Exception{
  EGLDisplay d=EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY);int[] version=new int[2];if(!EGL14.eglInitialize(d,version,0,version,1))throw new Exception("EGL init");
  EGLConfig[] configs=new EGLConfig[1];int[] count={0};EGL14.eglChooseConfig(d,new int[]{EGL14.EGL_RENDERABLE_TYPE,64,EGL14.EGL_SURFACE_TYPE,EGL14.EGL_PBUFFER_BIT,EGL14.EGL_RED_SIZE,8,EGL14.EGL_GREEN_SIZE,8,EGL14.EGL_BLUE_SIZE,8,EGL14.EGL_NONE},0,configs,0,1,count,0);if(count[0]!=1)throw new Exception("EGL config");
  EGLContext c=EGL14.eglCreateContext(d,configs[0],EGL14.EGL_NO_CONTEXT,new int[]{EGL14.EGL_CONTEXT_CLIENT_VERSION,3,EGL14.EGL_NONE},0);
  EGLSurface surface=EGL14.eglCreatePbufferSurface(d,configs[0],new int[]{EGL14.EGL_WIDTH,1,EGL14.EGL_HEIGHT,1,EGL14.EGL_NONE},0);
  if(!EGL14.eglMakeCurrent(d,surface,surface,c))throw new Exception("EGL current");
  try{
   int p=GLES30.glCreateProgram();GLES30.glAttachShader(p,shader(GLES30.GL_VERTEX_SHADER,"#version 300 es\nvoid main(){gl_Position=vec4(0,0,0,1);}"));GLES30.glAttachShader(p,shader(GLES30.GL_FRAGMENT_SHADER,"#version 300 es\nprecision highp float;out vec4 col;void main(){col=vec4(1,0,0,1);}"));GLES30.glLinkProgram(p);
   JSONObject report=new JSONObject().put("renderer",GLES30.glGetString(GLES30.GL_RENDERER)).put("version",GLES30.glGetString(GLES30.GL_VERSION)).put("source_linked",value(p,GLES30.GL_LINK_STATUS));
   int size=value(p,GLES30.GL_PROGRAM_BINARY_LENGTH);if(size<=0)throw new Exception("No binary");ByteBuffer binary=ByteBuffer.allocateDirect(size).order(ByteOrder.nativeOrder());int[] written={0},format={0};GLES30.glGetProgramBinary(p,size,written,0,format,0,binary);
   report.put("binary_length",written[0]).put("binary_format",format[0]).put("export_error",GLES30.glGetError());
   int loaded=GLES30.glCreateProgram();GLES30.glProgramBinary(loaded,format[0],binary,written[0]);report.put("import_error",GLES30.glGetError()).put("binary_linked",value(loaded,GLES30.GL_LINK_STATUS)).put("binary_log",GLES30.glGetProgramInfoLog(loaded));
   System.out.println(report.toString());GLES30.glDeleteProgram(p);GLES30.glDeleteProgram(loaded);
  }finally{EGL14.eglMakeCurrent(d,EGL14.EGL_NO_SURFACE,EGL14.EGL_NO_SURFACE,EGL14.EGL_NO_CONTEXT);EGL14.eglDestroySurface(d,surface);EGL14.eglDestroyContext(d,c);EGL14.eglTerminate(d);}
 }
}
