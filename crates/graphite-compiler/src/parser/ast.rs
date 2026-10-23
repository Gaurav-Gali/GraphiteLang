use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub imports: Vec<ImportDecl>,
    pub types: Vec<TypeDecl>,
    pub classes: Vec<ClassDecl>,
    pub functions: Vec<FunctionDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDecl {
    pub name: Identifier,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDecl {
    pub name: Identifier,
    pub fields: Vec<ClassField>,
    pub init: Option<ClassInit>,
    pub methods: Vec<ClassMethod>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassField {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    pub const_field: bool,
    pub public: bool,
    pub initializer: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassInit {
    pub public: bool,
    pub parameters: Vec<Parameter>,
    pub body: BlockStmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassMethod {
    pub public: bool,
    pub is_static: bool,
    pub return_type: TypeSyntax,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub body: BlockStmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDecl {
    pub domain: Identifier,
    pub library: Identifier,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDecl {
    pub return_type: TypeSyntax,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub body: BlockStmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    /// Nested parameters when this parameter is callable, as in `int op(int x)`.
    /// `Some` with an empty list is a zero-argument callable.
    pub callable: Option<Vec<Parameter>>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionMode {
    Exact,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    pub name: String,
    pub span: Span,
}

/// Maximum number of array dimensions a type can carry.
pub const MAX_ARRAY_RANK: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSyntax {
    pub kind: TypeKind,
    /// Set when `kind` is `Named`. Primitive types leave this empty.
    pub name: String,
    /// One entry per dimension, innermost first. `None` is a dynamic array.
    pub bounds: Vec<Option<u64>>,
    /// Key and value types when `kind` is `Map`.
    pub arguments: Vec<TypeSyntax>,
    /// `int?` and `node?`. The `?` applies to the whole type, after array bounds.
    pub optional: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Void,
    Int,
    UInt,
    Flt,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
    Flt32,
    Flt64,
    Bln,
    Chr,
    Str,
    Named,
    Map,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockStmt {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Block(BlockStmt),
    VariableDecl(VariableDecl),
    Destructure(DestructureDecl),
    Return(ReturnStmt),
    If(IfStmt),
    Loop(LoopStmt),
    Break(Span),
    Continue(Span),
    Try(TryStmt),
    Expression(ExpressionStmt),
    Conditional(ConditionalStmt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TryStmt {
    pub body: BlockStmt,
    pub catches: Vec<CatchClause>,
    pub finally_block: Option<BlockStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchClause {
    pub error_type: Identifier,
    pub binding: Identifier,
    pub block: BlockStmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructureDecl {
    pub bindings: Vec<BindingHead>,
    pub initializer: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingHead {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDecl {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    /// Absent when the variable starts at the type's default value.
    pub initializer: Option<Expr>,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TernaryExpr {
    pub condition: Box<Expr>,
    pub then_branch: Box<Expr>,
    pub else_branch: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LambdaExpr {
    pub ret: TypeSyntax,
    pub params: Vec<Parameter>,
    pub body: BlockStmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionStmt {
    pub expression: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalStmt {
    pub condition: Expr,
    pub action: Box<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_branch: BlockStmt,
    pub else_branch: Option<Box<Stmt>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopStmt {
    pub kind: LoopKind,
    pub body: BlockStmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopKind {
    Infinite,
    Condition {
        condition: Expr,
    },
    CStyle {
        initializer: Option<VariableDecl>,
        init_expr: Option<Expr>,
        condition: Option<Expr>,
        update: Option<Expr>,
    },
    Iteration {
        bindings: Vec<BindingHead>,
        iterable: Expr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Identifier(Identifier),
    Literal(LiteralExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Assignment(AssignmentExpr),
    Call(CallExpr),
    New(NewExpr),
    Conversion(ConversionExpr),
    Member(MemberExpr),
    Update(UpdateExpr),
    InterpolatedString(InterpolatedStringExpr),
    Array(ArrayExpr),
    Record(RecordExpr),
    Index(IndexExpr),
    Slice(SliceExpr),
    Ternary(TernaryExpr),
    Lambda(LambdaExpr),
    Parenthesized { expression: Box<Expr>, span: Span },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Identifier(identifier) => identifier.span,
            Expr::Literal(literal) => literal.span(),
            Expr::Unary(unary) => unary.span,
            Expr::Binary(binary) => binary.span,
            Expr::Assignment(assignment) => assignment.span,
            Expr::Call(call) => call.span,
            Expr::New(new_expr) => new_expr.span,
            Expr::Conversion(conversion) => conversion.span,
            Expr::Member(member) => member.span,
            Expr::Update(update) => update.span,
            Expr::InterpolatedString(interpolated) => interpolated.span,
            Expr::Array(array) => array.span,
            Expr::Record(record) => record.span,
            Expr::Index(index) => index.span,
            Expr::Slice(slice) => slice.span,
            Expr::Ternary(ternary) => ternary.span,
            Expr::Lambda(lambda) => lambda.span,
            Expr::Parenthesized { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralExpr {
    Integer { lexeme: String, span: Span },
    Float { lexeme: String, span: Span },
    Boolean { value: bool, span: Span },
    String { value: String, span: Span },
    Null { span: Span },
}

impl LiteralExpr {
    pub fn span(&self) -> Span {
        match self {
            LiteralExpr::Integer { span, .. }
            | LiteralExpr::Float { span, .. }
            | LiteralExpr::Boolean { span, .. }
            | LiteralExpr::String { span, .. }
            | LiteralExpr::Null { span } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayExpr {
    pub elements: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordExpr {
    pub entries: Vec<BraceEntry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BraceEntry {
    Pair { key: Expr, value: Expr, span: Span },
    Spread { value: Expr, span: Span },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexExpr {
    pub object: Box<Expr>,
    pub index: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceExpr {
    pub object: Box<Expr>,
    pub start: Option<Box<Expr>>,
    pub stop: Option<Box<Expr>>,
    pub step: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnaryExpr {
    pub operator: UnaryOperator,
    pub operand: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Negate,
    Not,
    BitNot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub operator: BinaryOperator,
    pub right: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Power,
    FloorDiv,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentExpr {
    pub target: Box<Expr>,
    pub value: Box<Expr>,
    /// `None` is plain `=`. `Some` is the operation in a compound assignment.
    pub op: Option<BinaryOperator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewExpr {
    pub name: Identifier,
    pub arguments: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub arguments: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionExpr {
    pub target: TypeSyntax,
    pub arguments: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberExpr {
    pub object: Box<Expr>,
    pub member: Identifier,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateExpr {
    pub operator: UpdateOperator,
    pub operand: Box<Expr>,
    pub position: UpdatePosition,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOperator {
    Increment,
    Decrement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePosition {
    Prefix,
    Postfix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpolatedStringExpr {
    pub parts: Vec<StringPart>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringPart {
    Text { value: String, span: Span },
    Expression { expression: Expr, span: Span },
}
